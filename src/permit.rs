use std::sync::{Mutex, TryLockError};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::Canceled;

use log::{warn, trace};

#[cfg(feature = "tokio-semaphore")]
mod tokio_semaphore;

#[cfg(feature = "tokio-semaphore")]
pub use tokio_semaphore::{
    blocking_permit_future,
    BlockingPermit,
    BlockingPermitFuture,
};

/// An async-aware semaphore for constraining the number of concurrent blocking
/// operations.
#[cfg(feature = "tokio-semaphore")]
pub use tokio::sync::Semaphore;

#[cfg(not(feature = "tokio-semaphore"))]
mod intrusive;

#[cfg(not(feature = "tokio-semaphore"))]
#[cfg(feature = "futures-intrusive")]
pub use intrusive::{
    blocking_permit_future,
    BlockingPermit,
    BlockingPermitFuture,
};

/// An async-aware semaphore for constraining the number of concurrent blocking
/// operations.
#[cfg(not(feature = "tokio-semaphore"))]
#[cfg(feature = "futures-intrusive")]
pub use futures_intrusive::sync::Semaphore;

// Note: these methods are common to both above struct definitions

impl<'a> BlockingPermit<'a> {
    /// Enter the blocking section of code on the current thread.
    ///
    /// This is a secondary step from completion of the
    /// [`BlockingPermitFuture`] as it must be call on the same thread,
    /// immediately before the blocking section.  The blocking permit should
    /// then be dropped at the end of the blocking section. If the
    /// _tokio-threaded_ feature is or might be used, `run` should be
    /// used instead.
    ///
    /// ## Panics
    ///
    /// Panics if this `BlockingPermit` has already been entered.
    pub fn enter(&self) {
        if self.entered.replace(true) {
            panic!("BlockingPermit::enter (or run) called twice!");
        }
    }

    /// Enter and run the blocking closure.
    ///
    /// This wraps the `tokio::task::block_in_place` call, as a secondary step
    /// from completion of the [`BlockingPermitFuture`] that must be call on
    /// the same thread. The permit is passed by value and will be dropped on
    /// termination of this call.
    ///
    /// ## Panics
    ///
    /// Panics if this `BlockingPermit` has already been entered.
    pub fn run<F, T>(self, f: F) -> T
        where F: FnOnce() -> T
    {
        if self.entered.replace(true) {
            panic!("BlockingPermit::run (or enter) called twice!");
        }

        #[cfg(feature="tokio-threaded")] {
            tokio::task::block_in_place(f)
        }

        #[cfg(not(feature="tokio-threaded"))] {
            f()
        }
    }
}

impl<'a> BlockingPermitFuture<'a> {
    /// Make a `Sync` version of this future by wrapping with a `Mutex`.
    pub fn make_sync(self) -> SyncBlockingPermitFuture<'a> {
        SyncBlockingPermitFuture {
            futr: Mutex::new(self)
        }
    }
}

impl<'a> Drop for BlockingPermit<'a> {
    fn drop(&mut self) {
        if self.entered.get() {
            trace!("Dropped BlockingPermit (semaphore)");
        } else {
            warn!("Dropped BlockingPermit (semaphore) was never entered")
        }
    }
}

/// A `Sync` wrapper available via [`BlockingPermitFuture::make_sync`].
#[must_use = "must be `.await`ed or polled"]
#[derive(Debug)]
pub struct SyncBlockingPermitFuture<'a> {
    futr: Mutex<BlockingPermitFuture<'a>>
}

impl<'a> SyncBlockingPermitFuture<'a> {
    /// Construct given `Semaphore` reference.
    pub fn new(semaphore: &'a Semaphore) -> SyncBlockingPermitFuture<'a>
    {
        SyncBlockingPermitFuture {
            futr: Mutex::new(BlockingPermitFuture::new(semaphore))
        }
    }
}

impl<'a> Future for SyncBlockingPermitFuture<'a> {
    type Output = Result<BlockingPermit<'a>, Canceled>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        match self.futr.try_lock() {
            Ok(mut guard) => {
                let futr = unsafe { Pin::new_unchecked(&mut *guard) };
                futr.poll(cx)
            }
            Err(TryLockError::Poisoned(_)) => Poll::Ready(Err(Canceled)),
            Err(TryLockError::WouldBlock) => {
                cx.waker().wake_by_ref(); //any spin should be brief
                Poll::Pending
            }
        }
    }
}
