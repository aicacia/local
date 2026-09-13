use std::io;

pub async fn run() -> io::Result<()> {
    Err(io::Error::other(
        "management-server requires a verified Global Identity runtime; use the unified idp-server or desktop application",
    ))
}
