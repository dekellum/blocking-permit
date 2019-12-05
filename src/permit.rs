use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_intrusive::sync::{SemaphoreAcquireFuture, SemaphoreReleaser};
use log::{info, trace};

#[cfg(feature="tokio_pool")]
use tokio_executor::threadpool as tokio_pool;

use crate::{Canceled, DispatchPool, IsReactorThread, Semaphore};

/// A scoped permit for blocking operations. When dropped (out of scope or
/// manually), the permit is released.
///
/// Note that [`enter`](BlockingPermit::enter) must be called before the actual
/// blocking begins.
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

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
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

impl<'a> BlockingPermit<'a> {
    /// Enter the blocking section of code on the current thread.
    ///
    /// This is a required secondary step from the [`BlockingPermitFuture`],
    /// and for consistency the [`blocking_permit`] call, because it _must_ be
    /// performed on the same thread, immediately before the blocking section.
    /// The blocking permit should then be dropped at the end of the blocking
    /// section.
    ///
    /// ## TODO
    ///
    /// This currently awaits access to a
    /// `tokio_executor::threadpool::enter_blocking_section` function or
    /// similar, until then use the `run` or `run_unwrap` methods which takes a
    /// closure.
    ///
    /// ## Panics
    ///
    /// Panics if this `BlockingPermit` has already been entered.
    pub fn enter(&self) {
        if !self.entered.replace(true) {
            // TODO: enter_blocking_section()
        } else {
            panic!("BlockingPermit::enter (or run) called twice!");
        }
    }

    // TODO: provide manual (ahead of drop) exit(), if also desired?

    /// Enter and run the blocking closure.
    ///
    /// This wraps the "legacy" `tokio_executor::threadpool::blocking` call
    /// with the same return value.  A caller may wish to panic (see
    /// [`run_unwrap`](BlockingPermit::run_unwrap) or propigate as an error,
    /// any result other than `Ready(Ok(T))`, for example:
    ///
    /// * `Poll::Pending` can be avoided if the tokio `ThreadPool` is
    ///    configured with `max_blocking` set greater than the sum of all
    ///    semaphore permits in use.  Setting `max_blocking` to 32768 minus the
    ///    pool size works currently.
    ///
    /// * `Poll::Ready(Err(BlockError))` occurs if the current thread is not a
    ///   Tokio concurrent `ThreadPool` worker. Don't do that! Use
    ///   `dispatch_blocking` instead.
    ///
    /// ## TODO
    ///
    /// Once tokio-threadpool is updated, this will be deprecated and emulated,
    /// then removed in favor of [`enter`](BlockingPermit::enter).
    ///
    /// ## Panics
    ///
    /// Panics if this `BlockingPermit` has already been entered (via `enter`
    /// or `run`*).
    #[cfg(feature="tokio_pool")]
    pub fn run<F, T>(&self, f: F)
        -> Poll<Result<T, tokio_pool::BlockingError>>
        where F: FnOnce() -> T
    {
        if self.entered.replace(true) {
            panic!("BlockingPermit::run (or enter) called twice!");
        }
        tokio_pool::blocking(f)
    }

    /// Enter and run the blocking closure, with confidence.
    ///
    /// This wraps the "legacy" `tokio_executor::threadpool::blocking`, unwraps
    /// and returns the value `v` of `Poll::Ready(Ok(v))`, type T, or panics on
    /// other returns.
    ///
    /// ## TODO
    ///
    /// Once tokio-threadpool is updated, this will be deprecated and
    /// emulated, then removed in favor of [`enter`](BlockingPermit::enter).
    ///
    /// ## Panics
    ///
    /// Panics on non-success returns. See [`run`](BlockingPermit::run) for
    /// ways to avoid these panics.
    ///
    /// Panics if this `BlockingPermit` has already been entered (via `enter`
    /// or `run`*).
    #[cfg(feature="tokio_pool")]
    pub fn run_unwrap<F, T>(&self, f: F) -> T
        where F: FnOnce() -> T
    {
        match self.run(f) {
            Poll::Ready(Ok(v)) => v,
            Poll::Ready(Err(e)) => {
                panic!("permit misused: {}", e)
            }
            Poll::Pending => {
                panic!("set tokio_threadpool max_blocking higher,\
                        or semaphore permits lower!")
            }
        }
    }
}

impl<'a> Drop for BlockingPermit<'a> {
    fn drop(&mut self) {
        let entered = self.entered.get();
        if entered {
            // TODO: exit_blocking_section()
        }
        if entered {
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
/// [`enter`](BlockingPermit::enter)ed to allow blocking or "long running"
/// operation, while the `BlockingPermit` remains in scope. If no permits are
/// immediately available, then the current task context will be notified when
/// one becomes available.
///
/// If the number of blocking threads need not be constrained by a `Semaphore`,
/// then this operation can be short circuited via [`blocking_permit`].
///
/// This returns an `IsReactorThread` error if the calling thread can't or
/// shouldn't become blocking, e.g. is a current thread runtime or is otherwise
/// registered to use a `DispatchPool`. This is recoverable by using
/// [`dispatch_blocking`](crate::dispatch_blocking) instead.
pub fn blocking_permit_future(semaphore: &Semaphore)
    -> Result<BlockingPermitFuture<'_>, IsReactorThread>
{
    if DispatchPool::is_thread_registered() {
        return Err(IsReactorThread);
    }

    Ok(BlockingPermitFuture {
        acquire: semaphore.acquire(1)
    })
}
