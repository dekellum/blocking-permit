use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(not(any(feature = "tokio-oneshot", feature = "futures-channel")))]
compile_error!("One of tokio-oneshot or futures-channel (default) \
                features is required for this crate!");

#[cfg(feature="tokio-oneshot")]
use tokio::sync::oneshot;

#[cfg(not(feature="tokio-oneshot"))]
use futures_channel::oneshot;

use crate::{Canceled, DispatchPool};

thread_local!(static POOL: RefCell<Option<DispatchPool>> = RefCell::new(None));

/// Register a `DispatchPool` on the calling thread.
///
/// Any prior instance is returned. This consumes the pool by value, but it can
/// be cloned beforehand to preserve an owned handle.
pub fn register_dispatch_pool(pool: DispatchPool) -> Option<DispatchPool> {
    POOL.with(|p| p.replace(Some(pool)))
}

/// Deregister and return any `DispatchPool` on the current thread.
pub fn deregister_dispatch_pool() -> Option<DispatchPool> {
    POOL.with(|p| p.replace(None))
}

/// Return true if a DispatchPool is registered to the current thread.
pub fn is_dispatch_pool_registered() -> bool {
    POOL.with(|p| p.borrow().is_some())
}

/// Dispatch a blocking operation closure to a pool, if registered.
///
/// If a pool has been registered via [`register_dispatch_pool`], then the
/// closure is spawned on the pool and this returns `None`. Otherwise the
/// original closure is returned.
///
/// Alternatively [`dispatch_rx`] may be used to await and obtain a return
/// value from the closure.
pub fn dispatch<F>(f: F) -> Option<F>
    where F: FnOnce() + Send + 'static
{
    POOL.with(|p| {
        if let Some(pool) = p.borrow().as_ref() {
            pool.spawn(Box::new(f));
            None
        } else {
            Some(f)
        }
    })
}

/// Value returned by [`dispatch_rx`].
#[must_use = "futures do nothing unless awaited or polled"]
pub enum DispatchRx<F, T> {
    Dispatch(Dispatched<T>),
    NotRegistered(F),
}

impl<F, T> DispatchRx<F, T> {
    /// Unwrap to the contained `Dispatched` future.
    ///
    /// ## Panics
    /// Panics if `NotRegistered`.
    pub fn unwrap(self) -> Dispatched<T> {
        match self {
            DispatchRx::Dispatch(disp) => disp,
            DispatchRx::NotRegistered(_) => {
                panic!("no BlockingPool was registered for this thread")
            }
        }
    }
}

/// Dispatch a blocking operation closure to a registered pool, returning
/// a future for awaiting the result.
///
/// If a pool has been registered via [`register_dispatch_pool`], then the
/// closure is spawned on the pool, and this returns a `Dispatched` future,
/// which resolves to the result of the closure. Otherwise the original closure
/// is returned.
pub fn dispatch_rx<F, T>(f: F) -> DispatchRx<F, T>
    where F: FnOnce() -> T + Send + 'static,
          T: Send + 'static
{
    POOL.with(|p| {
        if let Some(pool) = p.borrow().as_ref() {
            let (tx, rx) = oneshot::channel();
            pool.spawn(Box::new(|| {
                tx.send(f()).ok();
            }));
            DispatchRx::Dispatch(Dispatched(rx))
        } else {
            DispatchRx::NotRegistered(f)
        }
    })
}

/// A future type created by [`dispatch_rx`].
#[derive(Debug)]
pub struct Dispatched<T>(oneshot::Receiver<T>);

impl<T> Future for Dispatched<T> {
    type Output = Result<T, Canceled>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        match Future::poll(Pin::new(&mut self.0), cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(v)) => Poll::Ready(Ok(v)),
            Poll::Ready(Err(_)) => Poll::Ready(Err(Canceled))
        }
    }
}
