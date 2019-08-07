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

pub struct BlockingPermit {
    permit: Option<(Permit, &'static Semaphore)>,
}

impl Drop for BlockingPermit {
    fn drop(&mut self) {
        //DefaultExecutor::current().exit_blocking_section(&self);

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
pub fn blocking_permit(_semaphore: &'static Semaphore, _cx: &mut Context<'_>)
    -> Poll<Result<BlockingPermit, NotPermitted>>
{
    unimplemented!()
}

/// Request a permit to perform a blocking operation on the current thread,
/// unlimited by any maximum number of threads.  May still return
/// `NotPermitted::IsReactorThread`.
pub fn blocking_permit_unlimited() -> Result<BlockingPermit, NotPermitted>
{
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;

    pub mod tokio_fs {
        use lazy_static::lazy_static;
        use tokio_sync::semaphore::Semaphore;

        lazy_static! {
            pub static ref FILE_IO_SET: Semaphore = Semaphore::new(22);
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
    fn usage() {
    }
}
