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
/// current state, where [`BlockingPermit::run`] must be used, and will
/// eventually be deprecated.
#[cfg(feature="tokio_threaded")]
#[macro_export] macro_rules! permit_run_or_dispatch {
    ($semaphore:expr, $closure:expr) => {{
        let closure = $closure;
        match $crate::blocking_permit_future($semaphore) {
            Ok(f) => {
                let permit = f .await?;
                permit.run(closure)
            }
            Err($crate::IsReactorThread) => {
                $crate::dispatch_rx(closure) .await?
            }
        }
    }};
}
