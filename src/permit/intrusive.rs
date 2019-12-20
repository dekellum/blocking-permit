use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::future::FusedFuture;
use futures_intrusive::sync::{SemaphoreAcquireFuture, SemaphoreReleaser};

/// An async-aware semaphore for constraining the number of concurrent blocking
/// operations.
pub use futures_intrusive::sync::Semaphore;

use crate::{Canceled, Semaphorish};

/// A scoped permit for blocking operations. When dropped (out of scope or
/// manually), the permit is released.
#[must_use = "must call `run` or `enter` before blocking"]
#[derive(Debug)]
pub struct BlockingPermit<'a> {
    releaser: SemaphoreReleaser<'a>,
    pub(crate) entered: Cell<bool>
}

/// A future which resolves to a [`BlockingPermit`], created via the
/// [`blocking_permit_future`] function.
#[must_use = "must be `.await`ed or polled"]
#[derive(Debug)]
pub struct BlockingPermitFuture<'a> {
    semaphore: &'a Semaphore,
    acquire: Option<SemaphoreAcquireFuture<'a>>
}

impl Semaphorish for Semaphore {
    fn default_new(permits: usize) -> Self {
        Semaphore::new(true, permits)
    }
}

impl<'a> BlockingPermitFuture<'a> {
    /// Construct given `Semaphore` reference.
    pub fn new(semaphore: &Semaphore) -> BlockingPermitFuture<'_>
    {
        BlockingPermitFuture {
            semaphore,
            acquire: None
        }
    }
}

impl<'a> Future for BlockingPermitFuture<'a> {
    type Output = Result<BlockingPermit<'a>, Canceled>;

    // Note that with this implementation, `Canceled` is never returned. For
    // maximum future flexibilty however (like reverting to tokio's Semaphore)
    // we keep the error type in place.

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        // Safety: FIXME
        let this = unsafe { self.get_unchecked_mut() };
        let acq = if let Some(ref mut af) = this.acquire {
            af
        } else {
            this.acquire = Some(this.semaphore.acquire(1));
            this.acquire.as_mut().unwrap()
        };
        let acq = unsafe { Pin::new_unchecked(acq) };
        match acq.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(releaser) => Poll::Ready(Ok(BlockingPermit {
                releaser,
                entered: Cell::new(false)
            })),
       }
    }
}

impl<'a> FusedFuture for BlockingPermitFuture<'a> {
    fn is_terminated(&self) -> bool {
        if let Some(ref ff) = self.acquire {
            ff.is_terminated()
        } else {
            false
        }
    }
}

/// Request a permit to perform a blocking operation on the current thread.
///
/// The returned future attempts to obtain a permit from the provided
/// `Semaphore` and outputs a `BlockingPermit` which can then be
/// [`run`](BlockingPermit::run) to allow blocking or "long running"
/// operation, while the `BlockingPermit` remains in scope. If no permits are
/// immediately available, then the current task context will be notified when
/// one becomes available.
pub fn blocking_permit_future(semaphore: &Semaphore)
    -> BlockingPermitFuture<'_>
{
    BlockingPermitFuture::new(semaphore)
}
