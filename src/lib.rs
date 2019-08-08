// https://github.com/tokio-rs/tokio/issues/588
// https://github.com/tokio-rs/tokio/issues/1177

use std::task::{Context, Poll};
use std::thread;

use futures::channel::oneshot;

use tokio_sync::semaphore::{Permit, Semaphore};

/*
pub fn blocking<F, T>(f: F) -> Poll<Result<T, BlockingError>>
where
    F: FnOnce() -> T,
{
}

Returns:

Pending
  - CT: queue at capacity
  - MT: blocking capacity (run again)

Ready:
  - CT: queued (no result) <- How to garuntee ordering?
  - MT: run in blocking mode (with result)

*/

/// A scoped permit for blocking operations. When dropped (out of scope or
/// manually), the permit is released.
pub struct BlockingPermit<'a> {
    permit: Option<(Permit, &'a Semaphore)>,
}

impl<'a> Drop for BlockingPermit<'a> {
    fn drop(&mut self) {
        eprintln!("Dropping BlockingPermit!");

        // TODO: exit_blocking_section();

        if let Some((ref mut permit, ref semaphore)) = self.permit {
            permit.release(semaphore);
        }
    }
}

pub enum NotPermitted {
    /// The current thread is and must remain a reactor thread, where blocking
    /// is disallowed. The current thread runtime is in use.
    IsReactorThread,

    /// Dispatched task was canceled before it completed or awaited `Semaphore`
    /// closed before being acquired.
    Canceled,
}

/// Request a permit to perform a blocking operation on the current thread.
/// Attempts to obtain a permit from the given Semaphore, and if returned,
/// blocking operations are allowed while the `BlockingPermit` remains in
/// scope. If no permits are available, the current task context will be
/// notified when a permit becomes available.
pub fn blocking_permit<'a>(semaphore: &'a Semaphore, cx: &mut Context<'_>)
    -> Poll<Result<BlockingPermit<'a>, NotPermitted>>
{
    // TODO: Check if on current thread runtime?

    let mut permit = Permit::new();
    let ret = match permit.poll_acquire(cx, semaphore) {
        Poll::Pending => Poll::Pending,
        Poll::Ready(Ok(_)) => {
            eprintln!("Creating BlockingPermit (live)");
            Poll::Ready(Ok(BlockingPermit {
                permit: Some((permit, semaphore))
            }))
        },
        Poll::Ready(Err(_)) => Poll::Ready(Err(NotPermitted::Canceled)),
    };

    if let Poll::Ready(Ok(_)) = ret {
        // TODO: enter_blocking_section()
    }
    ret
}

/// Request a permit to perform a blocking operation on the current thread,
/// unlimited by any maximum number of threads.  May still return
/// `NotPermitted::IsReactorThread`.
pub fn blocking_permit_unlimited<'a>() -> Result<BlockingPermit<'a>, NotPermitted>
{
    // TODO: Check if on current thread runtime?

    // TODO: enter_blocking_section()

    eprintln!("Creating BlockingPermit (dead)");

    Ok(BlockingPermit { permit: None })
}

/// Use oneshot channel, and its error type, for our prototype custom Future
pub type DispatchBlocking<T> = futures::channel::oneshot::Receiver<T>;
pub type Canceled = futures::channel::oneshot::Canceled;

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

/*
pub struct DispatchBlocking<T> {
}

impl<T> Future for DispatchBlocking<T> {
    type Output = T;
}
*/

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;

    use futures::task::noop_waker;

    use super::*;

    pub mod tokio_fs {
        use lazy_static::lazy_static;
        use tokio_sync::semaphore::Semaphore;

        lazy_static! {
            pub static ref BLOCKING_SET: Semaphore = Semaphore::new(1);
        }
    }

    #[allow(dead_code)]
    struct TestFuture {
        dispatched: Option<DispatchBlocking<usize>>
    }

    impl TestFuture {
        #[allow(dead_code)]
        fn new() -> Self {
            TestFuture { dispatched: None }
        }
    }

    impl Future for TestFuture {
        type Output = Result<usize, Canceled>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
            if let Some(ref mut blocking) = self.dispatched {
                // FIXME: Unset self.dispatch on completion?
                return Pin::new(&mut *blocking).poll(cx);
            }
            match blocking_permit(&tokio_fs::BLOCKING_SET, cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Ok(_p)) => {
                    eprintln!("do some blocking stuff (41)");
                    Poll::Ready(Ok(41))
                }
                Poll::Ready(Err(NotPermitted::IsReactorThread)) => {
                    self.dispatched = Some(
                        dispatch_blocking( Box::new(|| -> usize {
                            eprintln!("do some blocking stuff (42)");
                            42
                        }))
                    );
                    // FIXME: Unset self.dispatch on (early)completion?
                    Pin::new(self.dispatched.as_mut().unwrap()).poll(cx)
                }
                Poll::Ready(Err(_)) => {
                    Poll::Ready(Err(Canceled {}))
                }
            }
        }
    }

    #[test]
    fn unlimited() {
        match blocking_permit_unlimited() {
            Ok(_p) => {
                eprintln!("do some blocking stuff");
            },
            Err(_e) => panic!("errored")
        }
    }

    #[test]
    fn limited() {
        let semaphore = Semaphore::new(1);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let permit1 = blocking_permit(&semaphore, &mut cx);
        match permit1 {
            Poll::Pending => panic!("pending?"),
            Poll::Ready(Ok(_)) => {
                eprintln!("do first blocking stuff");
            },
            Poll::Ready(Err(_e)) => panic!("errored")
        }
        let permit2 = blocking_permit(&semaphore, &mut cx);
        match permit2 {
            Poll::Pending => eprintln!("permits exhausted"),
            Poll::Ready(_) => panic!("permit2 ready?"),
        }
        drop(permit2);
        drop(permit1);
    }
}
