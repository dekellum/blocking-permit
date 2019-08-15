use tokio_sync::oneshot;

use crate::DispatchPool;

/// Dispatch a blocking operation in a closure to a non-reactor thread. Note
/// that [`dispatch_rx`] may be used to obtain the return value.
pub fn dispatch_blocking(f: Box<dyn FnOnce() + Send>)
{
    // TODO: Presumably with the concurrent runtime this should get queued on
    // an existing blocking thread (instead of requiring DispatchPool to be
    // registered for each concurrent runtime worker thread.
    DispatchPool::spawn_local(f);
}

/// A future type created by [`dispatch_rx`].
// TODO: For now just using tokio_sync::oneshot channel, its error type, and
// `Receiver` as a `Future`. Should this be wrapped with a new type?
pub type DispatchBlocking<T> = oneshot::Receiver<T>;

/// Dispatch a blocking operation in a closure via [`dispatch_blocking`], and
/// return a future representing its return value. This returns the
/// `DispatchBlocking<T>` type, where T is the return type of the closure.
pub fn dispatch_rx<F, T>(f: F) -> DispatchBlocking<T>
    where F: FnOnce() -> T + Send + 'static,
          T: Send + 'static
{
    let (tx, rx) = oneshot::channel();
    dispatch_blocking(Box::new(|| {
        tx.send(f()).ok();
    }));

    rx
}
