use alloc::boxed::Box;
use core::{future::Future, pin::Pin};

use crate::{StorageRequest, StorageResponse};

pub trait StorageSession: Send + Sync {
    fn execute_session(
        &self,
        request: StorageRequest,
    ) -> Pin<Box<dyn Future<Output = StorageResponse> + Send + '_>>;
}
