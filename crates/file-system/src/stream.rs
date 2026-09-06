use alloc::vec::Vec;
use core::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::Stream;

#[derive(Debug)]
pub struct ChunkStream {
    content: Vec<u8>,
    chunk_size: usize,
    offset: usize,
}

#[cfg(any(feature = "in-memory", feature = "native"))]
impl ChunkStream {
    pub(crate) fn new(content: Vec<u8>, chunk_size: usize) -> Self {
        Self {
            content,
            chunk_size,
            offset: 0,
        }
    }
}

impl Stream for ChunkStream {
    type Item = Vec<u8>;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.offset == self.content.len() {
            return Poll::Ready(None);
        }
        let end = self
            .offset
            .saturating_add(self.chunk_size)
            .min(self.content.len());
        let chunk = self.content[self.offset..end].to_vec();
        self.offset = end;
        Poll::Ready(Some(chunk))
    }
}
