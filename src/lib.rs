//! Experimental [`BlockingPermit`] and future, and an alternative
//! [`DispatchPool`] for enabling blocking operations on all types of
//! executors.
#![warn(rust_2018_idioms)]
#![feature(async_await)]

use log::warn;

mod dispatch;
mod dispatch_pool;

pub use dispatch::{dispatch_blocking, dispatch_rx, DispatchBlocking};

pub use dispatch_pool::{DispatchPool, DispatchPoolBuilder};

mod permit;

pub use permit::{
    BlockingPermit,
    BlockingPermitFuture,
    Canceled,
    IsReactorThread,
    blocking_permit,
    blocking_permit_future,
};

/// Attempt to obtain a permit for a blocking operation on thread, or
/// otherwise dispatch.
///
/// Helper macro for use in the context of an `async` block or function,
/// repeating the same code block in thread if [`blocking_permit_future`] (or
/// [`blocking_permit`]) succeeds, or via [`dispatch_rx`], if
/// [`IsReactorThread`] is returned.
///
/// ## Usage
///
/// If the first argument is a `Semaphore` reference, uses
/// [`blocking_permit_future`] with that `Semaphore`, otherwise uses
/// [`blocking_permit`] (unlimited). Also the return type of the _closure_ may
/// be optionally annotated.
///
/// TODO: usage examples/ doc-tests
///
/// ```rust no_compile no_run
/// permit_or_dispatch!(|| { /*.. blocking code..*/ });
/// permit_or_dispatch!(&semaphore, || { /*.. blocking code..*/ });
/// ```
#[macro_export] macro_rules! permit_or_dispatch {
    (|| $b:block) => {
        match blocking_permit() {
            Err(IsReactorThread) => {
                dispatch_rx(|| {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(permit) => {
                permit.enter();
                Ok({$b})
            }
        }
    };
    (|| -> $a:ty $b:block) => {
        match blocking_permit() {
            Err(IsReactorThread) => {
                dispatch_rx(|| -> $a {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(permit) => {
                permit.enter();
                Ok({$b})
            }
        }
    };
    ($c:expr, || $b:block) => {
        match blocking_permit_future($c) {
            Err(IsReactorThread) => {
                dispatch_rx(|| {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(f) => {
                let permit = f .await?;
                permit.enter();
                Ok({$b})
            }
        }
    };
    ($c:expr, || -> $a:ty $b:block) => {
        match blocking_permit_future($c) {
            Err(IsReactorThread) => {
                dispatch_rx(|| -> $a {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(f) => {
                let permit = f .await?;
                permit.enter();
                Ok({$b})
            }
        }
    };
}

/// Attempt to obtain a permit for a blocking operation on thread, or
/// otherwise dispatch (legacy version).
///
/// Helper macro for use in the context of an `async` block or function,
/// repeating the same code block in thread if [`blocking_permit_future`] (or
/// [`blocking_permit`]) succeeds, or via [`dispatch_rx`], if
/// [`IsReactorThread`] is returned.
///
/// This variant is for the tokio concurrent runtime (`ThreadPool`) in its
/// current state, where [`BlockingPermit::run_unwrap`] must be used, and will
/// eventually be deprecated.
#[macro_export] macro_rules! permit_run_or_dispatch {
    (|| $b:block) => {
        match blocking_permit() {
            Err(IsReactorThread) => {
                dispatch_rx(|| {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(permit) => {
                Ok(permit.run_unwrap(|| {$b}))
            }
        }
    };
    (|| -> $a:ty $b:block) => {
        match blocking_permit() {
            Err(IsReactorThread) => {
                dispatch_rx(|| -> $a {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(permit) => {
                Ok(permit.run_unwrap(|| -> $a {$b}))
            }
        }
    };
    ($c:expr, || $b:block) => {
        match blocking_permit_future($c) {
            Err(IsReactorThread) => {
                dispatch_rx(|| {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(f) => {
                let permit = f .await?;
                Ok(permit.run_unwrap(|| {$b}))
            }
        }
    };
    ($c:expr, || -> $a:ty $b:block) => {
        match blocking_permit_future($c) {
            Err(IsReactorThread) => {
                dispatch_rx(|| -> $a {$b})
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(f) => {
                let permit = f .await?;
                Ok(permit.run_unwrap(|| -> $a {$b}))
            }
        }
    };
}

#[cfg(test)]
mod tests;
