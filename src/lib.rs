#![warn(rust_2018_idioms)]
#![feature(async_await)]

use std::cell::Cell;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use log::{warn, debug};

use tokio_sync::semaphore::{Permit, Semaphore};
use tokio_threadpool;

mod dispatch;
mod dispatch_pool;

pub use dispatch::{dispatch_blocking, dispatch_rx, DispatchBlocking};

pub use dispatch_pool::{DispatchPool, DispatchPoolBuilder};

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
            // TODO: Or use a dedicated error for over `poll`ing?
            return Poll::Ready(Err(Canceled))
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
    /// Enter and run the blocking closure.
    ///
    /// This wraps the "legacy" `tokio_threadpool::blocking` call with the same
    /// return value.  A caller may wish to panic on or propigate as an error,
    /// any result other than `Ready(Ok(T))`, for example:
    ///
    /// * For `Pending`, if the tokio `ThreadPool` is configured with
    ///   `max_blocking` set greater than the sum of all semaphore permits in
    ///   use.  Setting `max_blocking` to `std::usize::max_value() >> 1` should
    ///   do the trick.
    ///
    /// * For `Ready(Ok(BlockError))`, if the current thread runtime is either
    ///   not in use or only `dispatch_blocking`, and not `run` is used in that
    ///   case.
    ///
    /// __TODO__: Once tokio-threadpool is updated, this will be deprecated and
    /// emulated, then removed in favor of [`enter`](BlockingPermit::enter)
    pub fn run<F, T>(&self, f: F)
        -> Poll<Result<T, tokio_threadpool::BlockingError>>
        where F: FnOnce() -> T
    {
        if self.entered.replace(true) {
            panic!("BlockingPermit::run (or enter) called twice!");
        }
        tokio_threadpool::blocking(f)
    }

    /// Enter the blocking section of code on the current thread.
    ///
    /// This is a required secondary step from the [`BlockingPermitFuture`],
    /// and for consistency the [`blocking_permit`] call, because it _must_ be
    /// performed on the same thread, immediately before the blocking section.
    /// The blocking permit should then be dropped at the end of the blocking
    /// section.
    ///
    /// TODO: this currently awaits access to a
    /// `tokio_threadpool::enter_blocking_section` function or similar, until
    /// then use the blocking method which takes a clojure.
    ///
    /// ## Panics
    ///
    /// If this `BlockingPermit` has already been entered.
    pub fn enter(&self) {
        if !self.entered.replace(true) {
            // TODO: enter_blocking_section()
        } else {
            panic!("BlockingPermit::enter (or run) called twice!");
        }
    }

    // TODO: provide manual (ahead of drop) exit(), if also desired?
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
/// using [`dispatch_blocking`] instead.
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
/// [`dispatch_blocking`] instead.
pub fn blocking_permit_future(semaphore: &Semaphore)
    -> Result<BlockingPermitFuture<'_>, IsReactorThread>
{
    // TODO: Really test if on the current thread runtime
    #[cfg(feature="current_thread")] {
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
/// possibility resource exhaustion, use `blocking_permit_future` (with a
/// Semaphore) or `dispatch_blocking` instead.
///
/// This returns an `IsReactorThread` error if the current thread can't become
/// blocking, e.g. is a current thread runtime. This is recoverable by using
/// [`dispatch_blocking`] instead.
pub fn blocking_permit<'a>() -> Result<BlockingPermit<'a>, IsReactorThread>
{
    // TODO: Really test if on the current thread runtime
    #[cfg(feature="current_thread")] {
        return Err(IsReactorThread);
    }

    debug!("Creating BlockingPermit (unlimited)");

    Ok(BlockingPermit {
        permit: None,
        entered: Cell::new(false)
    })
}

/// Helper macro for use in the context of an `async` block or function,
/// repeating the same code block in thread if [`blocking_permit_future`] (or
/// [`blocking_permit`]) succeeds, or via [`dispatch_rx`], if
/// [`IsReactorThread`] is returned.
///
/// ## Usage
///
/// If the first argument is a `Semaphore` reference, uses
/// [`blocking_permit_future`] with that `Semaphore`, otherwise uses
/// [`blocking_permit`] (unlimited). Also the return type of the _closure_ may
/// be optionally annotated.
///
/// TODO: usage examples/ doc-tests
///
/// ```rust no_compile no_run
/// permit_or_dispatch!(|| { /*.. blocking code..*/ });
/// permit_or_dispatch!(&semaphore, || { /*.. blocking code..*/ });
/// ```
#[macro_export] macro_rules! permit_or_dispatch {
    (|| $b:block) => {
        match blocking_permit() {
            Err(IsReactorThread) => {
                dispatch_rx(|| {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(permit) => {
                permit.enter();
                Ok($b)
            }
        }
    };
    (|| -> $a:ty $b:block) => {
        match blocking_permit() {
            Err(IsReactorThread) => {
                dispatch_rx(|| -> $a {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(permit) => {
                permit.enter();
                Ok($b)
            }
        }
    };
    ($c:expr, || $b:block) => {
        match blocking_permit_future($c) {
            Err(IsReactorThread) => {
                dispatch_rx(|| {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(f) => {
                let permit = f .await?;
                permit.enter();
                Ok($b)
            }
        }
    };
    ($c:expr, || -> $a:ty $b:block) => {
        match blocking_permit_future($c) {
            Err(IsReactorThread) => {
                dispatch_rx(|| -> $a {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(f) => {
                let permit = f .await?;
                permit.enter();
                Ok({$b})
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::panic::UnwindSafe;
    use std::pin::Pin;
    use std::thread;
    use std::time::Duration;

    use futures::executor::block_on;
    use futures::future::{FutureExt, TryFutureExt};
    use lazy_static::lazy_static;
    use log::info;
    use tokio_threadpool as runtime;

    use super::*;

    // TODO: Pretend for now that this is part of tokio-fs and also somehow
    // configurable.
    pub mod tokio_fs {
        use lazy_static::lazy_static;
        use tokio_sync::semaphore::Semaphore;

        lazy_static! {
            pub static ref BLOCKING_SET: Semaphore = Semaphore::new(1);
        }
    }

    fn is_send<T: Send>() -> bool { true }

    #[allow(dead_code)]
    fn is_unwind_safe<T: UnwindSafe>() -> bool { true }

    #[test]
    fn test_blocking_permit_traits() {
        assert!(is_send::<BlockingPermit<'_>>());

        // TODO: its not UnwindSafe because `semaphore::Permit` is not.  Does
        // it need to be?
        // assert!(is_unwind_safe::<BlockingPermit<'_>>());
    }

    fn log_init() {
        env_logger::builder().is_test(true).try_init().ok();
    }

    fn register_dispatch_pool() {
        let pool = DispatchPool::builder().pool_size(2).create();
        DispatchPool::register_thread_local(pool);
    }

    #[test]
    fn unlimited_current_thread() {
        log_init();
        register_dispatch_pool();
        match blocking_permit() {
            Ok(_) => panic!("should have errored"),
            Err(IsReactorThread) => {}
        }
    }

    #[test]
    fn unlimited_not_current_thread() {
        log_init();
        match blocking_permit() {
            Ok(p) => {
                p.enter();
                info!("do some blocking stuff");
            },
            Err(_e) => panic!("errored")
        }
    }

    /// Test of a manually-constructed future which uses
    /// `blocking_permit_future` and `dispatch_blocking`.
    struct TestFuture<'a> {
        delegate: Delegate<'a>,
    }

    enum Delegate<'a> {
        Dispatch(DispatchBlocking<usize>),
        Permit(BlockingPermitFuture<'a>),
        None
    }

    impl<'a> TestFuture<'a> {
        fn new() -> Self {
            TestFuture { delegate: Delegate::None }
        }
    }

    impl<'a> Future for TestFuture<'a> {
        type Output = Result<usize, Canceled>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>)
            -> Poll<Self::Output>
        {
            match self.delegate {
                Delegate::None => {
                    match blocking_permit_future(&tokio_fs::BLOCKING_SET) {
                        Err(IsReactorThread) => {
                            let s = "dispatched S".to_owned();
                            self.delegate = Delegate::Dispatch(
                                dispatch_rx(move || -> usize {
                                    info!("do some blocking stuff ({})", s);
                                    thread::sleep(Duration::from_millis(100));
                                    42
                                })
                            );
                        },
                        Ok(f) => {
                            self.delegate = Delegate::Permit(f);
                        }
                    }
                    self.poll(cx) // recurse once, with delegate in place
                                  // (needed for correct waking)
                }
                Delegate::Dispatch(ref mut db) => {
                    info!("delegate poll to DispatchBlocking");
                    Pin::new(&mut *db).poll(cx).map_err(|_| Canceled)
                }
                Delegate::Permit(ref mut pf) => {
                    info!("delegate poll to BlockingPermitFuture");
                    match Pin::new(&mut *pf).poll(cx) {
                        Poll::Pending => Poll::Pending,
                        Poll::Ready(Ok(p)) => {
                            p.enter();
                            info!("do some blocking stuff (permitted)");
                            Poll::Ready(Ok(41))
                        }
                        Poll::Ready(Err(_)) => Poll::Ready(Err(Canceled))
                    }
                }
            }
        }
    }

    #[test]
    fn manual_future() {
        log_init();
        let val = block_on(TestFuture::new()).expect("success");
        assert!(val == 41 || val == 42);
    }

    #[test]
    fn async_block_await() {
        log_init();
        // Note how async/await makes this a lot nicer than the above
        // `TestFuture` manual way.
        let task = async {
            match blocking_permit_future(&tokio_fs::BLOCKING_SET) {
                Err(IsReactorThread) => {
                    let mut _i = 0;
                    dispatch_rx(move || -> usize {
                        info!("do some blocking stuff (dispatched)");
                        _i = 1;
                        41
                    })
                        .map_err(|_| Canceled)
                        .await
                }
                Ok(f) => {
                    let permit = f .await?;
                    permit.enter();
                    info!("do some blocking stuff (permitted)");
                    Ok(42)
                }
            }
        };
        let val = block_on(task).expect("task success");
        assert!(val == 41 || val == 42);
    }

    #[test]
    fn async_block_await_unlimited() {
        log_init();
        let task = async {
            match blocking_permit() {
                Ok(permit) => {
                    permit.enter();
                    info!("do some blocking stuff (permitted)");
                    Ok(42)
                }
                Err(IsReactorThread) => {
                    dispatch_rx(|| -> usize {
                        info!("do some blocking stuff (dispatched)");
                        41
                    })
                        .map_err(|_| Canceled)
                        .await
                }
            }
        };
        let val = block_on(task).expect("task success");
        assert!(val == 41 || val == 42);
    }

    #[test]
    fn async_block_with_macro() {
        log_init();
        let task = async {
            permit_or_dispatch!(&tokio_fs::BLOCKING_SET, || {
                info!("do some blocking stuff, here or there");
                41
            })
        };
        let val = block_on(task).expect("task success");
        assert_eq!(val, 41);
    }

    #[test]
    fn async_block_with_macro_unlimited() {
        log_init();
        let task = async {
            permit_or_dispatch!(|| {
                info!("do some blocking stuff, here or there");
                41
            })
        };
        let val = block_on(task).expect("task success");
        assert_eq!(val, 41);
    }

    #[test]
    fn test_threaded() {
        log_init();
        lazy_static! {
            static ref TEST_SET: Semaphore = Semaphore::new(3);
        }

        let rt = runtime::Builder::new().pool_size(7).build();
        for _ in 0..1000 {
            rt.spawn(async {
                permit_or_dispatch!(&TEST_SET, || {
                    info!("do some blocking stuff, here or there");
                    41
                })
            }.map(|r| {
                assert_eq!(41, r.expect("permit_or_dispatch future"));
                ()
            }));
        }
        rt.shutdown_on_idle().wait();
    }
}
