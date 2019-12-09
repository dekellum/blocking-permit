//! [`BlockingPermit`] and future, and an alternative
//! [`DispatchPool`] for enabling blocking operations on all types of
//! executors.

#![warn(rust_2018_idioms)]

mod dispatch;
mod dispatch_pool;
mod errors;
mod permit;

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

pub use permit::{
    blocking_permit_future,
    BlockingPermit,
    BlockingPermitFuture,
};

/// An async-aware semaphore for constraining the number of concurrent blocking
/// operations.
///
pub use futures_intrusive::sync::Semaphore;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fs;
