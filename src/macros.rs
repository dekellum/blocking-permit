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
/// First calls [`blocking_permit_future`] with the given semaphore, and awaits
/// a permit.  The closure argument is for the blocking operation, which should
/// return a `Result<T, E>` where `From<Canceled>` is implemented for type
/// E. The return type of the closure may be annotated as necessary. The
/// closure may also be a `move` closure.
///
/// ```rust no_compile no_run
/// permit_or_dispatch!(&semaphore, || { /*.. blocking code..*/ });
/// ```
#[macro_export] macro_rules! permit_or_dispatch {
    ($semaphore:expr, $closure:expr) => {{
        let closure = $closure;
        if $crate::DispatchPool::is_thread_registered() {
            $crate::dispatch_rx(closure) .await?
        } else {
            let permit = $crate::blocking_permit_future($semaphore) .await?;
            #[cfg(not(feature="tokio_threaded"))] {
                permit.enter();
                closure()
            }
            #[cfg(feature="tokio_threaded")] {
                permit.run(closure)
            }
        }
    }};
}
