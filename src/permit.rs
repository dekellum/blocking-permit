use log::{warn, trace};

#[cfg(feature = "tokio-semaphore")]
mod tokio_semaphore;

#[cfg(feature = "tokio-semaphore")]
pub use tokio_semaphore::{
    blocking_permit_future,
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
    blocking_permit_future,
    BlockingPermit,
    BlockingPermitFuture,
    Semaphore,
    SyncBlockingPermitFuture,
};

pub trait Semaphorish {
    /// Construct given number of permits. Choose _fair_ if that is an option.
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

impl<'a> Drop for BlockingPermit<'a> {
    fn drop(&mut self) {
        if self.entered.get() {
            trace!("Dropped BlockingPermit (semaphore)");
        } else {
            warn!("Dropped BlockingPermit (semaphore) was never entered")
        }
    }
}
