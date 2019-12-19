use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex, TryLockError};
use std::task::{Context, Poll};

use futures_core::future::FusedFuture;
use futures_intrusive::sync::{SemaphoreAcquireFuture, SemaphoreReleaser};
use log::{warn, trace};

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
    semaphore: &'a Semaphore,
    acquire: Option<SemaphoreAcquireFuture<'a>>
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

    /// Make a `Sync` version of this future by wrapping with a `Mutex`.
    pub fn make_sync(self) -> SyncBlockingPermitFuture<'a> {
        SyncBlockingPermitFuture {
            futr: Mutex::new(self)
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

impl<'a> BlockingPermit<'a> {
    /// Enter the blocking section of code on the current thread.
    ///
    /// This is a secondary step from completion of the
    /// [`BlockingPermitFuture`] as it must be call on the same thread,
    /// immediately before the blocking section.  The blocking permit should
    /// then be dropped at the end of the blocking section. With the
    /// _tokio-threaded_ feature, `run` should be used instead.
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
    #[cfg(feature="tokio-threaded")]
    pub fn run<F, T>(self, f: F) -> T
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
            warn!("Dropped BlockingPermit (semaphore) was never entered")
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
        semaphore,
        acquire: None
    }
}
