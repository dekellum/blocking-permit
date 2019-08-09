// https://github.com/tokio-rs/tokio/issues/588
// https://github.com/tokio-rs/tokio/issues/1177

#![warn(rust_2018_idioms)]
#![feature(async_await)]

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::thread;

use tokio_sync::{
    oneshot,
    semaphore::{Permit, Semaphore},
};

/// A scoped permit for blocking operations. When dropped (out of scope or
/// manually), the permit is released.
#[must_use]
pub struct BlockingPermit<'a> {
    permit: Option<(Permit, &'a Semaphore)>,
    entered: AtomicBool
}

/// A future which resolves to a [`BlockingPermit`], created via the
/// [`blocking_permit_future`] function.
#[must_use]
pub struct BlockingPermitFuture<'a> {
    semaphore: &'a Semaphore,
    acquired: AtomicBool,
}

// TODO: Complete application of must_use attributes above or elsewhere

// TODO: Decide if the use of AtomicBool in the above (vs just bool) is really
// warranted. If it is, consider relaxing from SeqCst.

// TODO: Remove or replace all eprintln calls with `log` below.

impl<'a> Future for BlockingPermitFuture<'a> {
    type Output = Result<BlockingPermit<'a>, Canceled>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        if self.acquired.load(Ordering::SeqCst) {
            // TODO: Or use a dedicated error for over `poll`ing?
            return Poll::Ready(Err(Canceled))
        }

        let mut permit = Permit::new();
        match permit.poll_acquire(cx, self.semaphore) {
            Poll::Ready(Ok(())) => {
                if !self.acquired.swap(true, Ordering::SeqCst) {
                    eprintln!("Creating BlockingPermit (with permit)");
                    Poll::Ready(Ok(BlockingPermit {
                        permit: Some((permit, self.semaphore)),
                        entered: AtomicBool::new(false)
                    }))
                } else {
                    Poll::Ready(Err(Canceled))
                }
            }
            Poll::Ready(Err(_)) => Poll::Ready(Err(Canceled)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<'a> BlockingPermit<'a> {
    /// Enter the blocking section of code on the current thread.  This is a
    /// required secondary step from the [`BlockingPermitFuture`] and
    /// [`blocking_permit`] call, because it _must_ be performed on the same
    /// thread as the blocking section.  The blocking permit should then be
    /// dropped at the end of the blocking section.
    pub fn enter(&self) {
        if !self.entered.swap(true, Ordering::SeqCst) {
            // TODO: enter_blocking_section()
        } else {
            panic!("BlockingPermit::enter called twice!");
            // TODO: Or just make this a log warning?
        }
    }

    // TODO: provide manual (ahead of drop) exit(), if also desired?
}

impl<'a> Drop for BlockingPermit<'a> {
    fn drop(&mut self) {
        if let Some((ref mut permit, ref semaphore)) = self.permit {
            eprintln!("Dropping BlockingPermit, releasing semaphore");
            permit.release(semaphore);
        }
        if self.entered.load(Ordering::SeqCst) {
            // TODO: exit_blocking_section()
            eprintln!("Dropped (entered) BlockingPermit");
        } else {
            eprintln!("Dropped (never entered) BlockingPermit!");
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
    if false {
        return Err(IsReactorThread);
    }

    Ok(BlockingPermitFuture {
        semaphore,
        acquired: AtomicBool::new(false)
    })
}

/// Immediately return a permit which should then be
/// [`enter`](BlockingPermit::enter)ed to allow a blocking or "long running"
/// operation to be performed on the current thread.
///
/// This variant is unconstrained by any maximum allowed number of threads.
/// To avoid and unbounded number of blocking threads being created, and
/// possible resource exhaustion, use `blocking_permit_future` or
/// `dispatch_blocking` instead.
///
/// This returns an `IsReactorThread` error if the current thread can't become
/// blocking, e.g. is a current thread runtime. This is recoverable by using
/// [`dispatch_blocking`] instead.
pub fn blocking_permit<'a>() -> Result<BlockingPermit<'a>, IsReactorThread>
{
    // TODO: Really test if on the current thread runtime
    if false {
        return Err(IsReactorThread);
    }

    eprintln!("Creating BlockingPermit (unlimited)");

    Ok(BlockingPermit {
        permit: None,
        entered: AtomicBool::new(false)
    })
}

// TODO: For now just using tokio_sync::oneshot channel, its error type, and
// `Receiver` for our prototype custom `Future`. Should this be wrapped with a
// new type?

/// A future type created by [`dispatch_blocking`].
pub type DispatchBlocking<T> = oneshot::Receiver<T>;

/// Dispatch a blocking operation in the closure to a non-reactor thread, and
/// return a future representing its return value.
pub fn dispatch_blocking<T>(f: Box<dyn FnOnce() -> T + Send>)
    -> DispatchBlocking<T>
    where T: Send + 'static
{
    let (tx, rx) = oneshot::channel();

    thread::spawn(move || {
        let r = f();
        tx.send(r).ok();
    });

    rx
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::panic::UnwindSafe;
    use std::pin::Pin;
    use std::time::Duration;

    use futures::executor::block_on;
    use futures::future::TryFutureExt;

    use super::*;

    // TODO: Pretend for now that this is part of tokio-fs and also (somehow?)
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

    #[test]
    fn unlimited() {
        match blocking_permit() {
            Ok(p) => {
                p.enter();
                eprintln!("do some blocking stuff");
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
                            self.delegate = Delegate::Dispatch(
                                dispatch_blocking( Box::new(|| -> usize {
                                    eprintln!("do some blocking stuff (dispatched)");
                                    thread::sleep(Duration::from_millis(100));
                                    42
                                }))
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
                    eprintln!("delegate poll to DispatchBlocking");
                    Pin::new(&mut *db).poll(cx).map_err( |_| Canceled)
                }
                Delegate::Permit(ref mut pf) => {
                    eprintln!("delegate poll to BlockingPermitFuture");
                    match Pin::new(&mut *pf).poll(cx) {
                        Poll::Pending => Poll::Pending,
                        Poll::Ready(Ok(p)) => {
                            p.enter();
                            eprintln!("do some blocking stuff (permitted)");
                            Poll::Ready(Ok(41))
                        }
                        Poll::Ready(Err(_)) => Poll::Ready(Err(Canceled))
                    }
                }
            }
        }
    }

    #[test]
    fn future() {
        let val = block_on(TestFuture::new()).expect("success");
        assert!(val == 41 || val == 42);
    }

    #[test]
    fn async_block_await() {
        // Note how async/await makes this a lot nicer than the above
        // `TestFuture` manual way.
        let task = async {
            match blocking_permit_future(&tokio_fs::BLOCKING_SET) {
                Err(IsReactorThread) => {
                    dispatch_blocking( Box::new(|| -> usize {
                        eprintln!("do some blocking stuff (dispatched)");
                        41
                    }))
                        .map_err( |_| Canceled)
                        .await
                }
                Ok(f) => {
                    let permit = f.await?;
                    permit.enter();
                    eprintln!("do some blocking stuff (permitted)");
                    Ok(42)
                }
            }
        };
        let val = block_on(task).expect("task success");
        assert!(val == 41 || val == 42);
    }
}
