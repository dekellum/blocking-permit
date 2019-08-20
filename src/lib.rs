//! Experimental [`BlockingPermit`] and future, and an alternative
//! [`DispatchPool`] for enabling blocking operations on all types of
//! executors.

#![warn(rust_2018_idioms)]
#![feature(async_await)]

mod dispatch;
mod dispatch_pool;
mod permit;

pub use dispatch::{dispatch_blocking, dispatch_rx, DispatchBlocking};

pub use dispatch_pool::{DispatchPool, DispatchPoolBuilder};

pub use permit::{
    blocking_permit,
    blocking_permit_future,
    BlockingPermit,
    BlockingPermitFuture,
    Canceled,
    IsReactorThread,
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
/// [`blocking_permit`] (unlimited).
///
/// The final and required argument is a closure over the blocking operation,
/// which should return a `Result<T, E>` where `From<Canceled>` is implemented
/// for type E. The return type of the closure may be annotated as
/// necessary. The closure may also be a move closure.
///
/// TODO: usage examples/ doc-tests
///
/// ```rust no_compile no_run
/// permit_or_dispatch!(|| { /*.. blocking code..*/ });
/// permit_or_dispatch!(&semaphore, || { /*.. blocking code..*/ });
/// ```
#[macro_export] macro_rules! permit_or_dispatch {
    ($b:expr) => {{
        let b = $b;
        match blocking_permit() {
            Ok(permit) => {
                permit.enter();
                b()
            }
            Err(IsReactorThread) => {
                dispatch_rx(b) .await?
            }
        }
    }};
    ($c:expr, $b:expr) => {{
        let b = $b;
        match blocking_permit_future($c) {
            Ok(f) => {
                let permit = f .await?;
                permit.enter();
                b()
            }
            Err(IsReactorThread) => {
                dispatch_rx(b) .await?
            }
        }
    }};
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
#[cfg(feature="tokio_pool")]
#[macro_export] macro_rules! permit_run_or_dispatch {
    ($b:expr) => {{
        let b = $b;
        match blocking_permit() {
            Ok(permit) => {
                permit.run_unwrap(b)
            }
            Err(IsReactorThread) => {
                dispatch_rx(b) .await?
            }
        }
    }};
    ($c:expr, $b:expr) => {{
        let b = $b;
        match blocking_permit_future($c) {
            Ok(f) => {
                let permit = f .await?;
                permit.run_unwrap(b)
            }
            Err(IsReactorThread) => {
                dispatch_rx(b) .await?
            }
        }
    }};
}

#[cfg(test)]
mod tests;
