use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};

use crate::{Error, Storage};

#[derive(Debug, Default)]
pub struct InMemoryStorage {
    files: BTreeMap<String, Vec<u8>>,
}

impl InMemoryStorage {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Storage for InMemoryStorage {
    type Error = Error;

    async fn read(&self, path: &str) -> Result<Vec<u8>, Self::Error> {
        validate_file_path(path)?;
        self.files.get(path).cloned().ok_or(Error::NotFound)
    }

    async fn write(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error> {
        validate_file_path(path)?;
        self.files.insert(path.to_string(), content.to_vec());
        Ok(())
    }

    async fn append(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error> {
        validate_file_path(path)?;
        self.files
            .get_mut(path)
            .ok_or(Error::NotFound)?
            .extend_from_slice(content);
        Ok(())
    }

    async fn remove(&mut self, path: &str) -> Result<(), Self::Error> {
        validate_file_path(path)?;
        self.files.remove(path).map(|_| ()).ok_or(Error::NotFound)
    }

    async fn rename(&mut self, from: &str, to: &str) -> Result<(), Self::Error> {
        validate_file_path(from)?;
        validate_file_path(to)?;
        let content = self.files.remove(from).ok_or(Error::NotFound)?;
        self.files.insert(to.to_string(), content);
        Ok(())
    }

    async fn scan(&self, path: &str) -> Result<Vec<String>, Self::Error> {
        validate_directory_path(path)?;
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}/")
        };
        Ok(self
            .files
            .keys()
            .filter(|file| file.starts_with(&prefix))
            .cloned()
            .collect())
    }
}

fn validate_file_path(path: &str) -> Result<(), Error> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err(Error::InvalidPath);
    }
    validate_components(path)
}

fn validate_directory_path(path: &str) -> Result<(), Error> {
    if path.is_empty() {
        return Ok(());
    }
    if path.starts_with('/') || path.ends_with('/') {
        return Err(Error::InvalidPath);
    }
    validate_components(path)
}

fn validate_components(path: &str) -> Result<(), Error> {
    if path
        .split('/')
        .all(|name| !name.is_empty() && name != "." && name != "..")
    {
        Ok(())
    } else {
        Err(Error::InvalidPath)
    }
}

#[cfg(test)]
mod tests {
    use super::InMemoryStorage;
    use crate::{Error, Storage};

    #[tokio::test]
    async fn performs_raw_file_operations() {
        let mut storage = InMemoryStorage::new();
        storage.write("notes/today.txt", b"hello").await.unwrap();
        storage.append("notes/today.txt", b" world").await.unwrap();
        storage
            .rename("notes/today.txt", "notes/tomorrow.txt")
            .await
            .unwrap();

        assert_eq!(
            storage.read("notes/tomorrow.txt").await.unwrap(),
            b"hello world"
        );
        assert_eq!(storage.scan("notes").await.unwrap(), ["notes/tomorrow.txt"]);
        storage.remove("notes/tomorrow.txt").await.unwrap();
        assert_eq!(
            storage.read("notes/tomorrow.txt").await,
            Err(Error::NotFound)
        );
    }
}
