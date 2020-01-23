use log::{warn, trace};

#[cfg(feature = "tokio-semaphore")]
mod tokio_semaphore;

#[cfg(feature = "tokio-semaphore")]
pub use tokio_semaphore::{
    BlockingPermit,
    BlockingPermitFuture,
    Semaphore,
    SyncBlockingPermitFuture,
};

#[cfg(not(feature = "tokio-semaphore"))]
#[cfg(feature = "futures-intrusive")]
mod intrusive;

#[cfg(not(feature = "tokio-semaphore"))]
#[cfg(feature = "futures-intrusive")]
pub use intrusive::{
    BlockingPermit,
    BlockingPermitFuture,
    Semaphore,
    SyncBlockingPermitFuture,
};

/// Extension trait for uniform construction of the re-exported [`Semaphore`]
/// types.
pub trait Semaphorish {
    /// Construct given number of permits.
    ///
    /// This chooses _fair_ scheduling, if that is a supported option.
    fn default_new(permits: usize) -> Self;
}

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
    /// from completion of the [`BlockingPermitFuture`] that must be called on
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
#[must_use = "blocking_permit_future does nothing unless polled/`await`-ed"]
pub fn blocking_permit_future(semaphore: &Semaphore)
    -> BlockingPermitFuture<'_>
{
    BlockingPermitFuture::new(semaphore)
}
