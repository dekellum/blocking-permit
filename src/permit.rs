use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::future::FusedFuture;
use futures_intrusive::sync::{SemaphoreAcquireFuture, SemaphoreReleaser};
use log::{info, trace};

use crate::{Canceled, Semaphore};

/// A scoped permit for blocking operations. When dropped (out of scope or
/// manually), the permit is released.
#[must_use = "must call `enter` before blocking"]
#[derive(Debug)]
pub struct BlockingPermit<'a> {
    releaser: SemaphoreReleaser<'a>,
    entered: Cell<bool>
}

/// A future which resolves to a [`BlockingPermit`], created via the
/// [`blocking_permit_future`] function.
#[must_use = "must be `.await`ed or polled"]
#[derive(Debug)]
pub struct BlockingPermitFuture<'a> {
    acquire: SemaphoreAcquireFuture<'a>
}

impl<'a> Future for BlockingPermitFuture<'a> {
    type Output = Result<BlockingPermit<'a>, Canceled>;

    // Note that with this implementation, `Canceled` is never returned. For
    // maximum future flexibilty however (like reverting to tokio's Semaphore)
    // we keep the error type in place.

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        // Safety: the self Pin means our self address is stable, and acquire
        // is furthermore never moved.
        let acq = unsafe {
            Pin::new_unchecked(&mut self.get_unchecked_mut().acquire)
        };
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
        self.acquire.is_terminated()
    }
}

impl<'a> BlockingPermit<'a> {
    /// Enter the blocking section of code on the current thread.
    ///
    /// This is a required secondary step from completion of the
    /// [`BlockingPermitFuture`] as it _must_ be performed on the same thread,
    /// immediately before the blocking section.  The blocking permit should
    /// then be dropped at the end of the blocking section.
    ///
    /// ## TODO
    ///
    /// Currently this only used for internal testing to mimimally satisfy an
    /// enter. As yet unclear if it should be retained and ultimately exported.
    ///
    /// ## Panics
    ///
    /// Panics if this `BlockingPermit` has already been entered.
    #[cfg(test)]
    pub(crate) fn enter(&self) {
        if self.entered.replace(true) {
            panic!("BlockingPermit::enter (or run) called twice!");
        }
    }

    /// Enter and run the blocking closure.
    ///
    /// This wraps the `tokio::task::block_in_place` call.
    ///
    /// ## Panics
    ///
    /// Panics if this `BlockingPermit` has already been entered.
    #[cfg(feature="tokio_threaded")]
    pub fn run<F, T>(&self, f: F) -> T
        where F: FnOnce() -> T
    {
        if self.entered.replace(true) {
            panic!("BlockingPermit::run (or enter) called twice!");
        }
        tokio::task::block_in_place(f)
    }
}

impl<'a> Drop for BlockingPermit<'a> {
    fn drop(&mut self) {
        if self.entered.get() {
            trace!("Dropped BlockingPermit (semaphore)");
        } else {
            info!("Dropped BlockingPermit (semaphore) was never entered")
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
    BlockingPermitFuture {
        acquire: semaphore.acquire(1)
    }
}
