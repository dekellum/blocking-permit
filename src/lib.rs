//! Experimental [`BlockingPermit`] and future, and an alternative
//! [`DispatchPool`] for enabling blocking operations on all types of
//! executors.

#![warn(rust_2018_idioms)]
#![feature(async_await)]

mod dispatch;
mod dispatch_pool;
mod errors;
mod permit;

pub use dispatch::{dispatch_blocking, dispatch_rx, DispatchBlocking};

pub use dispatch_pool::{DispatchPool, DispatchPoolBuilder};

pub use errors::{Canceled, IsReactorThread};

pub use permit::{
    blocking_permit,
    blocking_permit_future,
    BlockingPermit,
    BlockingPermitFuture,
};

/// Attempt to obtain a permit for a blocking operation on thread, or
/// otherwise dispatch.
///
/// Helper macro for use in the context of an `async` block or function,
/// running a closure in thread if [`blocking_permit_future`] (or
/// [`blocking_permit`]) succeeds, or via [`dispatch_rx`], if
/// [`IsReactorThread`] is returned.
///
/// ## Usage
///
/// If the first argument is a `Semaphore` reference, uses
/// [`blocking_permit_future`] for that semaphore, otherwise uses
/// [`blocking_permit`] (unlimited).
///
/// The final, required argument is a closure over the blocking operation,
/// which should return a `Result<T, E>` where `From<Canceled>` is implemented
/// for type E. The return type of the closure may be annotated as
/// necessary. The closure may also be a `move` closure.
///
/// TODO: usage examples/ doc-tests
///
/// ```rust no_compile no_run
/// permit_or_dispatch!(|| { /*.. blocking code..*/ });
/// permit_or_dispatch!(&semaphore, || { /*.. blocking code..*/ });
/// ```
#[macro_export] macro_rules! permit_or_dispatch {
    ($closure:expr) => {{
        let closure = $closure;
        match $crate::blocking_permit() {
            Ok(permit) => {
                permit.enter();
                closure()
            }
            Err($crate::IsReactorThread) => {
                $crate::dispatch_rx(closure) .await?
            }
        }
    }};
    ($semaphore:expr, $closure:expr) => {{
        let closure = $closure;
        match $crate::blocking_permit_future($semaphore) {
            Ok(f) => {
                let permit = f .await?;
                permit.enter();
                closure()
            }
            Err($crate::IsReactorThread) => {
                $crate::dispatch_rx(closure) .await?
            }
        }
    }};
}

/// Attempt to obtain a permit for a blocking operation on thread, or
/// otherwise dispatch (legacy version).
///
/// Helper macro for use in the context of an `async` block or function,
/// running a closure in thread if [`blocking_permit_future`] (or
/// [`blocking_permit`]) succeeds, or via [`dispatch_rx`], if
/// [`IsReactorThread`] is returned.
///
/// This variant is for the tokio concurrent runtime (`ThreadPool`) in its
/// current state, where [`BlockingPermit::run_unwrap`] must be used, and will
/// eventually be deprecated.
#[cfg(feature="tokio_pool")]
#[macro_export] macro_rules! permit_run_or_dispatch {
    ($closure:expr) => {{
        let closure = $closure;
        match $crate::blocking_permit() {
            Ok(permit) => {
                permit.run_unwrap(closure)
            }
            Err($crate::IsReactorThread) => {
                $crate::dispatch_rx(closure) .await?
            }
        }
    }};
    ($semaphore:expr, $closure:expr) => {{
        let closure = $closure;
        match $crate::blocking_permit_future($semaphore) {
            Ok(f) => {
                let permit = f .await?;
                permit.run_unwrap(closure)
            }
            Err($crate::IsReactorThread) => {
                $crate::dispatch_rx(closure) .await?
            }
        }
    }};
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fs;
