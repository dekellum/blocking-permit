use std::fmt;
use std::io;

/// Error type returned as output from the
/// [`BlockingPermitFuture`](crate::BlockingPermitFuture) or
/// [`DispatchBlocking`](crate::DispatchBlocking) futures if they are canceled.
///
/// This only occurs if the associated `Semaphore` is closed or `DispatchPool`
/// is dropped, respectively.
#[derive(Debug)]
pub struct Canceled;

/// Error returned by [`blocking_permit_future`](crate::blocking_permit_future)
/// if the calling thread can't or shouldn't become blocking, e.g. is a current
/// thread runtime or is otherwise registered to use a `DispatchPool`.
///
/// This is recoverable by using
/// [`dispatch_blocking`](crate::dispatch_blocking) instead.
#[derive(Debug)]
pub struct IsReactorThread;

impl fmt::Display for Canceled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Waiting for a blocking permit or dispatch was canceled")
    }
}

impl std::error::Error for Canceled {}

impl From<Canceled> for io::Error {
    fn from(me: Canceled) -> io::Error {
        io::Error::new(io::ErrorKind::Other, me)
    }
}

impl fmt::Display for IsReactorThread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Shouldn't block the current thread (runtime?); \
                   use dispatch instead")
    }
}

impl std::error::Error for IsReactorThread {}
