use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::Storage;

#[derive(Debug)]
pub struct NativeStorage {
    root: PathBuf,
}

impl NativeStorage {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn path(&self, path: &str) -> io::Result<PathBuf> {
        validate_file_path(path)?;
        Ok(self.root.join(path))
    }
}

impl Storage for NativeStorage {
    type Error = io::Error;

    fn read(&self, path: &str) -> Result<Vec<u8>, Self::Error> {
        fs::read(self.path(path)?)
    }

    fn write(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error> {
        let path = self.path(path)?;
        let parent = path
            .parent()
            .ok_or_else(|| invalid_input("path has no parent"))?;
        fs::create_dir_all(parent)?;
        fs::write(path, content)
    }

    fn append(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error> {
        use std::io::Write;

        let path = self.path(path)?;
        let mut file = fs::OpenOptions::new().append(true).open(path)?;
        file.write_all(content)
    }

    fn remove(&mut self, path: &str) -> Result<(), Self::Error> {
        fs::remove_file(self.path(path)?)
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), Self::Error> {
        let from = self.path(from)?;
        let to = self.path(to)?;
        let parent = to
            .parent()
            .ok_or_else(|| invalid_input("path has no parent"))?;
        fs::create_dir_all(parent)?;
        fs::rename(from, to)
    }

    fn scan(&self, path: &str) -> Result<Vec<String>, Self::Error> {
        validate_directory_path(path)?;
        let root = if path.is_empty() {
            self.root.clone()
        } else {
            self.root.join(path)
        };
        let mut files = Vec::new();
        scan_directory(&self.root, &root, &mut files)?;
        files.sort();
        Ok(files)
    }
}

fn scan_directory(root: &Path, directory: &Path, files: &mut Vec<String>) -> io::Result<()> {
    match fs::read_dir(directory) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    scan_directory(root, &path, files)?;
                } else if path.is_file() {
                    let relative = path
                        .strip_prefix(root)
                        .map_err(|_| invalid_input("path escapes storage root"))?;
                    files.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn validate_file_path(path: &str) -> io::Result<()> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err(invalid_input(
            "path must name a file within the storage root",
        ));
    }
    validate_components(path)
}

fn validate_directory_path(path: &str) -> io::Result<()> {
    if path.is_empty() {
        return Ok(());
    }
    if path.starts_with('/') || path.ends_with('/') {
        return Err(invalid_input(
            "path must name a directory within the storage root",
        ));
    }
    validate_components(path)
}

fn validate_components(path: &str) -> io::Result<()> {
    if path
        .split('/')
        .all(|name| !name.is_empty() && name != "." && name != "..")
    {
        Ok(())
    } else {
        Err(invalid_input("path contains an invalid component"))
    }
}

fn invalid_input(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::NativeStorage;
    use crate::Storage;

    #[test]
    fn persists_raw_files() {
        let root = env::temp_dir().join(format!("file-system-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut storage = NativeStorage::new(&root).unwrap();
        storage.write("notes/today.txt", b"hello").unwrap();
        drop(storage);

        let storage = NativeStorage::new(&root).unwrap();
        assert_eq!(storage.read("notes/today.txt").unwrap(), b"hello");
        let _ = fs::remove_dir_all(root);
    }
}
