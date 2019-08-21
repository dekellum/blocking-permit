use std::fmt;

/// Error type returned as output from the [`BlockingPermitFuture`] or
/// [`DispatchBlocking`](crate::DispatchBlocking) futures if they are canceled.
///
/// This only occurs if the associated `Semaphore` is closed or `DispatchPool`
/// is dropped, respectively.
#[derive(Debug)]
pub struct Canceled;

/// Error returned by [`blocking_permit_future`] if the current thread is a
/// fixed reactor thread, e.g. current thread runtime. This is recoverable by
/// using [`dispatch_blocking`](crate::dispatch_blocking) instead.
#[derive(Debug)]
pub struct IsReactorThread;

impl fmt::Display for Canceled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Waiting for a blocking permit or dispatch was canceled")
    }
}

impl std::error::Error for Canceled {}

impl fmt::Display for IsReactorThread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Can't block a fixed reactor thread, \
                   e.g. current thread runtime, \
                   must dispatch instead)")
    }
}

impl std::error::Error for IsReactorThread {}
