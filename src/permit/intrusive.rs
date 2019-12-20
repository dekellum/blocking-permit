use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex, TryLockError};
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
    // maximum future flexibilty, however, we keep the error type in place.

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        let this = unsafe { self.get_unchecked_mut() };
        let acq = if let Some(ref mut af) = this.acquire {
            af
        } else {
            this.acquire = Some(this.semaphore.acquire(1));
            this.acquire.as_mut().unwrap()
        };

        // Safety: In this projection we Pin the underlying Future for the
        // duration of `poll` and it is not further moved.
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
