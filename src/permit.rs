use std::cell::Cell;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use log::{warn, debug};

use tokio_sync::semaphore::{Permit, Semaphore};

#[cfg(feature="tokio_pool")]
use tokio_executor::threadpool as tokio_pool;

use crate::DispatchPool;

/// A scoped permit for blocking operations. When dropped (out of scope or
/// manually), the permit is released.
///
/// Note that [`enter`](BlockingPermit::enter) must be called before the actual
/// blocking begins.
#[must_use = "must call `enter` before blocking"]
pub struct BlockingPermit<'a> {
    permit: Option<(Permit, &'a Semaphore)>,
    entered: Cell<bool>
}

/// A future which resolves to a [`BlockingPermit`], created via the
/// [`blocking_permit_future`] function.
#[must_use = "must be `.await`ed or polled"]
pub struct BlockingPermitFuture<'a> {
    semaphore: &'a Semaphore,
    permit: Option<Permit>,
    acquired: bool,
}

impl<'a> Future for BlockingPermitFuture<'a> {
    type Output = Result<BlockingPermit<'a>, Canceled>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        if self.acquired {
            panic!("BlockingPermitFuture::poll called again after acquired");
        }

        let mut permit = self.permit.take().unwrap_or_else(Permit::new);
        match permit.poll_acquire(cx, self.semaphore) {
            Poll::Ready(Ok(())) => {
                debug!("Creating BlockingPermit (semaphore)");
                self.acquired = true;
                Poll::Ready(Ok(BlockingPermit {
                    permit: Some((permit, self.semaphore)),
                    entered: Cell::new(false)
                }))
            }
            Poll::Ready(Err(_)) => Poll::Ready(Err(Canceled)),
            Poll::Pending => {
                self.permit = Some(permit);
                Poll::Pending
            }
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
                panic!("misused: {}", e)
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
        if let Some((ref mut permit, ref semaphore)) = self.permit {
            permit.release(semaphore);
            if entered {
                debug!("Dropped BlockingPermit (semaphore)");
            } else {
                warn!("Dropped BlockingPermit (semaphore) was never entered")
            }
        } else if entered {
            debug!("Dropped BlockingPermit (unlimited)");
        } else {
            warn!("Dropped BlockingPermit (unlimited) was never entered")
        }
    }
}

/// Error returned as output from [`BlockingPermitFuture`] if canceled, e.g. if
/// the associated `Semaphore` is closed.
#[derive(Debug)]
pub struct Canceled;

impl fmt::Display for Canceled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Waiting for a blocking permit was canceled")
    }
}

impl std::error::Error for Canceled {}

/// Error returned by [`blocking_permit_future`] if the current thread is a
/// fixed reactor thread, e.g. current thread runtime. This is recoverable by
/// using [`dispatch_blocking`](crate::dispatch_blocking) instead.
#[derive(Debug)]
pub struct IsReactorThread;

impl fmt::Display for IsReactorThread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Can't block a fixed reactor thread, \
                   e.g. current thread runtime, \
                   must dispatch instead)")
    }
}

impl std::error::Error for IsReactorThread {}

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
/// This returns an `IsReactorThread` error if the current thread can't become
/// blocking, e.g. is a current thread runtime. This is recoverable by using
/// [`dispatch_blocking`](crate::dispatch_blocking) instead.
pub fn blocking_permit_future(semaphore: &Semaphore)
    -> Result<BlockingPermitFuture<'_>, IsReactorThread>
{
    // TODO: Change the check when available, or refine the error name and
    // message. This makes `DispatchPool` drop-in applicable, but the error
    // is potentially confusing.
    if DispatchPool::is_thread_registered() {
        return Err(IsReactorThread);
    }

    Ok(BlockingPermitFuture {
        semaphore,
        permit: None,
        acquired: false,
    })
}

/// Immediately return a permit which should then be
/// [`enter`](BlockingPermit::enter)ed to allow a blocking or "long running"
/// operation to be performed on the current thread.
///
/// This variant is unconstrained by any maximum allowed number of threads. To
/// avoid an unbounded number of blocking threads from being created, and
/// possible resource exhaustion, use `blocking_permit_future` (with a
/// Semaphore) or `dispatch_blocking` instead.
///
/// This returns an `IsReactorThread` error if the current thread can't become
/// blocking, e.g. is a current thread runtime. This is recoverable by using
/// [`dispatch_blocking`](crate::dispatch_blocking) instead.
pub fn blocking_permit<'a>() -> Result<BlockingPermit<'a>, IsReactorThread>
{
    // TODO: Change the check when available, or refine the error name and
    // message. This makes `DispatchPool` drop-in applicable, but the error
    // is potentially confusing.
    if DispatchPool::is_thread_registered() {
        return Err(IsReactorThread);
    }

    debug!("Creating BlockingPermit (unlimited)");

    Ok(BlockingPermit {
        permit: None,
        entered: Cell::new(false)
    })
}
