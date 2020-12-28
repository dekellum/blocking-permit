use std::cell::Cell;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use log::debug;

use tokio::sync::{SemaphorePermit, AcquireError};

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

/// A future which resolves to a [`BlockingPermit`].
#[must_use = "futures do nothing unless awaited or polled"]
pub struct BlockingPermitFuture<'a> {
    semaphore: &'a Semaphore,
    permit: Option<Pin<Box<
            dyn Future<Output=Result<SemaphorePermit<'a>, AcquireError>>
            + Send + Sync + 'a
            >>>,
    acquired: bool,
}

impl Semaphorish for Semaphore {
    fn default_new(permits: usize) -> Self {
        Semaphore::new(permits)
    }
}

impl<'a> BlockingPermitFuture<'a> {

    /// Construct given `Semaphore` reference.
    pub fn new(semaphore: &'a Semaphore) -> BlockingPermitFuture<'a> {
        BlockingPermitFuture {
            semaphore,
            permit: None,
            acquired: false,
        }
    }

    /// Ensure a `Sync` version of this future.
    pub fn make_sync(self) -> SyncBlockingPermitFuture<'a>
        where Self: Sync
    {
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

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        let this = self.get_mut();

        if this.acquired {
            panic!("BlockingPermitFuture::poll called again after acquired");
        }

        let permit = if let Some(ref mut pt) = this.permit {
            pt.as_mut()
        } else {
            this.permit = Some(Box::pin(this.semaphore.acquire()));
            this.permit.as_mut().unwrap().as_mut()
        };

        match permit.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(spr) => {
                debug!("Creating BlockingPermit (semaphore)");
                this.acquired = true;
                if let Ok(p) = spr {
                    Poll::Ready(Ok(BlockingPermit {
                        permit: p,
                        entered: Cell::new(false)
                    }))
                } else {
                    Poll::Ready(Err(Canceled))
                }
            }
        }
    }
}
