//! Experimental [`BlockingPermit`] and future, and an alternative
//! [`DispatchPool`] for enabling blocking operations on all types of
//! executors.

#![warn(rust_2018_idioms)]

mod dispatch;
mod dispatch_pool;
mod errors;
mod permit;

#[macro_use] mod macros;

pub use dispatch::{dispatch_blocking, dispatch_rx, DispatchBlocking};

pub use dispatch_pool::{DispatchPool, DispatchPoolBuilder};

pub use errors::{Canceled, IsReactorThread};

pub use permit::{
    blocking_permit,
    blocking_permit_future,
    BlockingPermit,
    BlockingPermitFuture,
};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fs;
