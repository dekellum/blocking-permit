use std::cell::Cell;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use log::debug;

use tokio::sync::SemaphorePermit;

/// An async-aware semaphore for constraining the number of concurrent blocking
/// operations.
pub use tokio::sync::Semaphore;

use crate::{Canceled, Semaphorish};

/// A scoped permit for blocking operations. When dropped (out of scope or
/// manually), the permit is released.
///
/// Note that [`enter`](BlockingPermit::enter) must be called before the actual
/// blocking begins.
#[must_use = "must call `run` or `enter` before blocking"]
#[derive(Debug)]
pub struct BlockingPermit<'a> {
    permit: SemaphorePermit<'a>,
    pub(crate) entered: Cell<bool>
}

/// Alias for a guaranteed `Sync` version of the BlockingPermitFuture
pub type SyncBlockingPermitFuture<'a> = BlockingPermitFuture<'a>;

/// A future which resolves to a [`BlockingPermit`], created via the
/// [`blocking_permit_future`] function.
#[must_use = "must be `.await`ed or polled"]
pub struct BlockingPermitFuture<'a> {
    semaphore: &'a Semaphore,
    permit: Option<Box<dyn Future<Output=SemaphorePermit<'a>> + Send + Sync + 'a>>,
    acquired: bool,
}

impl Semaphorish for Semaphore {
    fn default_new(permits: usize) -> Self {
        Semaphore::new(permits)
    }
}

impl<'a> BlockingPermitFuture<'a> {

    /// Construct given `Semaphore` reference.
    pub fn new(semaphore: &'a Semaphore) -> BlockingPermitFuture<'a>
    {
        BlockingPermitFuture {
            semaphore,
            permit: None,
            acquired: false,
        }
    }

    /// Ensure a `Sync` version of this future.
    pub fn make_sync(self) -> SyncBlockingPermitFuture<'a> {
        self
    }

}

impl<'a> fmt::Debug for BlockingPermitFuture<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockingPermitFuture")
            .field("semaphore", self.semaphore)
            .field("permit", &self.permit.is_some())
            .field("acquired", &self.acquired)
            .finish()
    }
}

impl<'a> Future for BlockingPermitFuture<'a> {
    type Output = Result<BlockingPermit<'a>, Canceled>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        if self.acquired {
            panic!("BlockingPermitFuture::poll called again after acquired");
        }

        let mut permit = self.permit.take()
            .unwrap_or_else(|| Box::new(self.semaphore.acquire()));

        let ppin = unsafe { Pin::new_unchecked(&mut *permit) };
        match ppin.poll(cx) {
            Poll::Ready(sp) => {
                debug!("Creating BlockingPermit (semaphore)");
                self.acquired = true;
                Poll::Ready(Ok(BlockingPermit {
                    permit: sp,
                    entered: Cell::new(false)
                }))
            }
            Poll::Pending => {
                self.permit = Some(permit);
                Poll::Pending
            }
        }
    }
}

/// Request a permit to perform a blocking operation on the current thread.
///
/// The returned future attempts to obtain a permit from the provided
/// `Semaphore` and outputs a `BlockingPermit` which can then be
/// [`run`](BlockingPermit::run) to allow blocking or "long running" operation,
/// while the `BlockingPermit` remains in scope. If no permits are immediately
/// available, then the current task context will be notified when one becomes
/// available.
pub fn blocking_permit_future(semaphore: &Semaphore)
    -> BlockingPermitFuture<'_>
{
    BlockingPermitFuture::new(semaphore)
}
