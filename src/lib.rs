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
//!
//! * A [`Cleaver`] for splitting `Stream` buffers into more manageable sizes.
//!
//! ## Optional Features
//!
//! The following features may be enabled at build time. **All are disabled by
//! default, unless otherwise noted.**
//!
//! _futures-channel_
//! : Use this oneshot channel implementation (Default
//!   enabled, but overridden by _tokio-oneshot_ or _tokio-omnibus_.)
//!
//! _tokio-oneshot_
//! : Use tokio's oneshot channel implementation (Overrides _futures-channel_
//!   default.).
//!
//! _futures-intrusive_
//! : Include `BlockingPermit` and re-export `Semaphore` from the
//!   _futures-intrusive_ crate. (Works with all prominent runtimes.)
//!
//! _tokio-semaphore_
//! : Include `BlockingPermit` and re-export tokio's `Semaphore`
//!   type. (Overrides _futures-intrusive_.)
//!
//! _tokio-threaded_
//! : Add `block_in_place` support, exposed via [`BlockingPermit::run`].
//!
//! _tokio-omnibus_
//! : A simpler way to include all above and, we expect, any future added
//!   _tokio-*_ features in this crate.
//!
//! _cleaver_
//! : Include the [`Cleaver`] wrapper stream.

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
    Semaphorish,
    SyncBlockingPermitFuture,
};

#[cfg(feature = "cleaver")]
mod cleaver;

#[cfg(feature = "cleaver")]
pub use cleaver::{
    Cleaver,
    Splittable,
};

/// An async-aware semaphore for constraining the number of concurrent blocking
/// operations.
///
/// This re-exported type is either `futures_intrusive::sync::Semaphore`
/// (_futures-intrusive_ feature) or `tokio::sync::Semaphore`
/// (_tokio-semaphore_ or _tokio-omnibus_ features).
///
/// ----------
///
#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#[doc(inline)]
pub use permit::Semaphore;

#[cfg(test)]
mod tests;

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#[cfg(test)]
mod fs;
