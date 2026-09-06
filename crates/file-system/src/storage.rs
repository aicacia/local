use alloc::{string::String, vec::Vec};

pub trait Storage {
    type Error;

    fn read(&self, path: &str) -> Result<Vec<u8>, Self::Error>;
    fn write(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error>;
    fn append(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error>;
    fn remove(&mut self, path: &str) -> Result<(), Self::Error>;
    fn rename(&mut self, from: &str, to: &str) -> Result<(), Self::Error>;
    fn scan(&self, path: &str) -> Result<Vec<String>, Self::Error>;
}
