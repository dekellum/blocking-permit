// https://github.com/tokio-rs/tokio/issues/588
// https://github.com/tokio-rs/tokio/issues/1177

use std::task::{Context, Poll};

use tokio_sync::{
    semaphore,
    semaphore::{Permit, Semaphore}
};

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

    /// Error acquiring a permit from the provided `Semaphore`, e.g. the
    /// Semaphore has been closed.
    OnAcquire(semaphore::AcquireError),
}

/// Request a permit to perform a blocking operation on the current thread.
/// Attempts to obtain a permit from the given Semaphore. If no permits are
/// available, the current task context will be notified when a permit is
/// available.
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
        Poll::Ready(Err(e)) => Poll::Ready(Err(NotPermitted::OnAcquire(e))),
    };

    // TODO: enter_blocking_section()
    ret
}

/// Request a permit to perform a blocking operation on the current thread,
/// unlimited by any maximum number of threads.  May still return
/// `NotPermitted::IsReactorThread`.
pub fn blocking_permit_unlimited<'a>() -> Result<BlockingPermit<'a>, NotPermitted>
{
    // TODO: DefaultExecutor::current().enter_blocking_section()
    eprintln!("Creating BlockingPermit (dead)");

    Ok(BlockingPermit { permit: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;

    use futures::task::noop_waker;

    pub mod tokio_fs {
        use lazy_static::lazy_static;
        use tokio_sync::semaphore::Semaphore;

        lazy_static! {
            pub static ref FILE_IO_SET: Semaphore = Semaphore::new(1);
        }
    }

    #[allow(dead_code)]
    struct TestFuture;

    impl Future for TestFuture {
        type Output = Result<(), NotPermitted>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
            match blocking_permit(&tokio_fs::FILE_IO_SET, cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Ok(_p)) => {
                    // blocking operation(s), might panic
                    return Poll::Ready(Ok(()));
                }
                Poll::Ready(Err(NotPermitted::IsReactorThread)) => {
                    /*
                    self.dispatched = Some(
                        DefaultExecutor::current().dispatch_blocking( Box::new(|| {
                        // blocking operation(s)
                        }))
                    );
                    */
                    return Poll::Pending;
                }
                Poll::Ready(Err(e)) => {
                    return Poll::Ready(Err(e))
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
