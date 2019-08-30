use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio_sync::oneshot;

use crate::{Canceled, DispatchPool};

/// Dispatch a blocking operation in a closure to a non-reactor thread.
///
/// Note that [`dispatch_rx`] may be used to obtain the return value.
///
/// ## Panics
///
/// Currently this relies on [`DispatchPool::register_thread_local`] to have been
/// previously called on the calling thread, and will panic otherwise.
pub fn dispatch_blocking(f: Box<dyn FnOnce() + Send>)
{
    // TODO: Presumably with the concurrent runtime this should get queued on
    // an existing blocking thread (instead of requiring DispatchPool to be
    // registered for each concurrent runtime worker thread.
    DispatchPool::spawn_registered(f);
}

/// Dispatch a blocking operation in a closure via [`dispatch_blocking`], and
/// return a future representing its return value.
///
/// This returns the `DispatchBlocking<T>` type, where T is the return type of
/// the closure.
///
/// ## Panics
///
/// Currently this relies on [`DispatchPool::register_thread_local`] to have been
/// previously called on the calling thread, and will panic otherwise.
pub fn dispatch_rx<F, T>(f: F) -> DispatchBlocking<T>
    where F: FnOnce() -> T + Send + 'static,
          T: Send + 'static
{
    let (tx, rx) = oneshot::channel();
    dispatch_blocking(Box::new(|| {
        tx.send(f()).ok();
    }));

    DispatchBlocking(rx)
}

/// A future type created by [`dispatch_rx`].
#[derive(Debug)]
pub struct DispatchBlocking<T>(oneshot::Receiver<T>);

impl<T> Future for DispatchBlocking<T> {
    type Output = Result<T, Canceled>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Future::poll(Pin::new(&mut self.0), cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(v)) => Poll::Ready(Ok(v)),
            Poll::Ready(Err(_)) => Poll::Ready(Err(Canceled))
        }
    }
}
