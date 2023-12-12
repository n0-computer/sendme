use std::io;

use bytes::Bytes;
use futures::{future::LocalBoxFuture, FutureExt};
use iroh_io::AsyncSliceWriter;

/// A slice writer that adds a synchronous progress callback
#[derive(Debug)]
pub struct ProgressSliceWriter2<W, F>(W, F);

#[allow(dead_code)]
impl<W: AsyncSliceWriter, F: Fn(u64, usize) -> io::Result<()> + 'static>
    ProgressSliceWriter2<W, F>
{
    /// Create a new `ProgressSliceWriter` from an inner writer and a progress callback
    pub fn new(inner: W, on_write: F) -> Self {
        Self(inner, on_write)
    }

    /// Return the inner writer
    pub fn into_inner(self) -> W {
        self.0
    }
}

impl<W: AsyncSliceWriter + 'static, F: Fn(u64, usize) -> io::Result<()> + 'static> AsyncSliceWriter
    for ProgressSliceWriter2<W, F>
{
    type WriteBytesAtFuture<'a> = LocalBoxFuture<'a, io::Result<()>>;
    fn write_bytes_at(&mut self, offset: u64, data: Bytes) -> Self::WriteBytesAtFuture<'_> {
        // todo: get rid of the boxing
        async move {
            (self.1)(offset, data.len())?;
            self.0.write_bytes_at(offset, data).await
        }
        .boxed_local()
    }

    type WriteAtFuture<'a> = W::WriteAtFuture<'a>;
    fn write_at<'a>(&'a mut self, offset: u64, bytes: &'a [u8]) -> Self::WriteAtFuture<'a> {
        self.0.write_at(offset, bytes)
    }

    type SyncFuture<'a> = W::SyncFuture<'a>;
    fn sync(&mut self) -> Self::SyncFuture<'_> {
        self.0.sync()
    }

    type SetLenFuture<'a> = W::SetLenFuture<'a>;
    fn set_len(&mut self, size: u64) -> Self::SetLenFuture<'_> {
        self.0.set_len(size)
    }
}
