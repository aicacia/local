use alloc::{string::String, vec::Vec};
use core::future::Future;

pub trait Storage {
    type Error;

    fn read(&self, path: &str) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send;
    fn write(
        &mut self,
        path: &str,
        content: &[u8],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn append(
        &mut self,
        path: &str,
        content: &[u8],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn remove(&mut self, path: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn rename(
        &mut self,
        from: &str,
        to: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn scan(&self, path: &str) -> impl Future<Output = Result<Vec<String>, Self::Error>> + Send;
}
