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
