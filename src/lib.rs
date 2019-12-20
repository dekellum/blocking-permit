//! This crate provides:
//!
//! * A specialized, custom thread pool, [`DispatchPool`], for offloading
//!   blocking or otherwise long running operations from a main or reactor
//!   thread(s). Once registered, it is used via [`dispatch_rx()`] (to await a
//!   return value) or [`dispatch()`] for background tasks (fire and forget).
//!
//! * A [`BlockingPermit`], obtained via [`blocking_permit_future()`] for
//!   limiting the number of concurrent blocking operations via a re-exported
//!   [`Semaphore`] type selected by one of the (non-default) features
//!   _futures-intrusive_, or _tokio-semaphore_ (or _tokio-omnibus_).

#![warn(rust_2018_idioms)]

mod dispatch;
mod dispatch_pool;
mod errors;

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
mod permit;

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#[macro_use] mod macros;

pub use dispatch::{
    dispatch,
    dispatch_rx,
    is_dispatch_pool_registered,
    register_dispatch_pool,
    deregister_dispatch_pool,
    Dispatched,
    DispatchRx,
};

pub use dispatch_pool::{DispatchPool, DispatchPoolBuilder};

pub use errors::Canceled;

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
pub use permit::{
    blocking_permit_future,
    BlockingPermit,
    BlockingPermitFuture,
    Semaphore,
    Semaphorish,
    SyncBlockingPermitFuture,
};

#[cfg(test)]
mod tests;

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#[cfg(test)]
mod fs;
