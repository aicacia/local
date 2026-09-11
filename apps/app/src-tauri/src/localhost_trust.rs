use std::{io, path::Path};

pub fn install_ca_to_user_trust_store(ca_pem_path: &Path) -> io::Result<()> {
    #[cfg(test)]
    {
        let _ = ca_pem_path;
        Ok(())
    }

    #[cfg(not(test))]
    match install_ca_to_user_trust_store_impl(ca_pem_path) {
        Ok(()) => {
            log::info!("installed localhost CA into the operating system trust store");
            Ok(())
        }
        Err(err) => {
            log::warn!(
                "failed to install localhost CA into user trust store: {err}. open the trust page in your browser to accept the certificate manually."
            );
            Err(err)
        }
    }
}

#[cfg(not(test))]
fn install_ca_to_user_trust_store_impl(ca_pem_path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        return install_ca_macos(ca_pem_path);
    }

    #[cfg(target_os = "windows")]
    {
        return install_ca_windows(ca_pem_path);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        install_ca_linux(ca_pem_path)
    }
}

#[cfg(all(not(test), target_os = "macos"))]
fn install_ca_macos(ca_pem_path: &Path) -> io::Result<()> {
    let home = std::env::var("HOME").map_err(io::Error::other)?;
    let keychain = format!("{home}/Library/Keychains/login.keychain-db");
    let status = Command::new("security")
        .args([
            "add-trusted-cert",
            "-r",
            "trustRoot",
            "-p",
            "ssl",
            "-k",
            &keychain,
            &ca_pem_path.to_string_lossy(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            "security add-trusted-cert failed for localhost CA",
        ))
    }
}

#[cfg(all(not(test), target_os = "windows"))]
fn install_ca_windows(ca_pem_path: &Path) -> io::Result<()> {
    let status = Command::new("certutil")
        .args(["-addstore", "-user", "Root", &ca_pem_path.to_string_lossy()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            "certutil failed to install localhost CA into user Root store",
        ))
    }
}

#[cfg(all(not(test), not(any(target_os = "macos", target_os = "windows"))))]
fn install_ca_linux(ca_pem_path: &Path) -> io::Result<()> {
    if command_succeeds("trust", &["anchor", &ca_pem_path.to_string_lossy()]) {
        return Ok(());
    }

    let system_cert = "/usr/local/share/ca-certificates/aicacia-localhost-ca.crt";
    if command_succeeds(
        "pkexec",
        &[
            "install",
            "-m",
            "0644",
            &ca_pem_path.to_string_lossy(),
            system_cert,
        ],
    ) && (command_succeeds("pkexec", &["update-ca-certificates"])
        || command_succeeds("pkexec", &["update-ca-trust", "extract"]))
    {
        return Ok(());
    }

    install_ca_linux_nss_fallback(ca_pem_path).map_err(|_| {
        io::Error::other(
            "could not install the CA into the Linux OS trust store; approve the elevation prompt or install it manually",
        )
    })
}

#[cfg(all(not(test), not(any(target_os = "macos", target_os = "windows"))))]
fn command_succeeds(command: &str, args: &[&str]) -> bool {
    use std::process::{Command, Stdio};

    Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(all(not(test), not(any(target_os = "macos", target_os = "windows"))))]
fn install_ca_linux_nss_fallback(ca_pem_path: &Path) -> io::Result<()> {
    if !command_succeeds("certutil", &["-H"]) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "certutil unavailable",
        ));
    }

    let home = std::env::var("HOME").map_err(io::Error::other)?;
    let home = Path::new(&home);
    let candidates = [
        home.join(".pki/nssdb"),
        home.join(".var/app/com.brave.Browser/.pki/nssdb"),
        home.join(".var/app/com.google.Chrome/data/pki/nssdb"),
        home.join(".var/app/org.chromium.Chromium/.pki/nssdb"),
    ];

    for database in candidates {
        if database.is_dir() && install_ca_in_nss_database(&database, ca_pem_path).is_ok() {
            return Ok(());
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no NSS database available",
    ))
}

#[cfg(all(not(test), not(any(target_os = "macos", target_os = "windows"))))]
fn install_ca_in_nss_database(database: &Path, ca_pem_path: &Path) -> io::Result<()> {
    use std::process::{Command, Stdio};

    let database = format!("sql:{}", database.display());
    let nickname = "Localhost CA";

    let _ = Command::new("certutil")
        .args(["-d", &database, "-D", "-n", nickname])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let status = Command::new("certutil")
        .args([
            "-d",
            &database,
            "-A",
            "-t",
            "C,,",
            "-n",
            nickname,
            "-i",
            &ca_pem_path.to_string_lossy(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("certutil failed to install localhost CA"))
    }
}
