/// Attempt to dispatch a blocking operation, or otherwise obtain a permit and
/// run on thread, returning the result of the closure.
///
/// This helper macro is intended for use in the context of an `async` block or
/// function. It first attempts to dispatch the closure via [`dispatch_rx`] and
/// await. If a dispatch pool is not registered on the current thread, it
/// instead obtains a permit via [`blocking_permit_future`] and awaits.  If the
/// _tokio_threaded_ feature is enabled, it will then run the closure via
/// [`BlockingPermit::run`]. Otherwise it will run the closure directly.
///
/// ## Usage
///
/// The closure should return a `Result<T, E>` where `From<Canceled>` is
/// implemented for type E. The return type of the closure may be annotated as
/// necessary. The closure may also be a `move` closure.
///
/// ```rust no_compile no_run
/// async {
///     dispatch_or_permit!(&semaphore, move || { /*.. blocking code..*/ });
/// }
/// ```
#[macro_export] macro_rules! dispatch_or_permit {
    ($semaphore:expr, $closure:expr) => {{
        match $crate::dispatch_rx($closure) {
            $crate::DispatchRx::Dispatch(disp) => {
                disp .await?
            }
            $crate::DispatchRx::NotRegistered(cl) => {
                let permit = $crate::blocking_permit_future($semaphore) .await?;
                #[cfg(not(feature="tokio_threaded"))] {
                    permit.enter();
                    cl()
                }
                #[cfg(feature="tokio_threaded")] {
                    permit.run(cl)
                }
            }
        }
    }};
}
