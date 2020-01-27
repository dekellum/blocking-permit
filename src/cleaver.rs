use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::stream::Stream;

/// Trait for buffer types that may be split at a maximum length.
///
/// This is enabled via the *cleaver* feature.
pub trait Splittable: Sized {
    /// Split if larger than a maximum length.
    ///
    /// If self has length greater than the specified maximum, split it into
    /// two, returning the new leading segment and with self mutated to
    /// contain the remainder.
    fn split_if(&mut self, max: usize) -> Option<Self>;
}

/// This implementation is inexpensive, relying on `Bytes::split_to` which does
/// not copy.
impl Splittable for Bytes {
    fn split_if(&mut self, max: usize) -> Option<Self> {
        if self.len() > max {
            Some(self.split_to(max))
        } else {
            None
        }
    }
}

/// A `Stream` adapter that splits buffers from a source to a given, maximum
/// length.
///
/// This may be useful to limit the amount of time spent processing each `Item`
/// of a `Splittable` stream.  This is enabled via the *cleaver* feature.
#[must_use = "streams do nothing unless polled"]
pub struct Cleaver<B, E, St>
    where B: Splittable + Unpin,
          St: Stream<Item=Result<B, E>>
{
    source: St,
    rem: Option<B>,
    max: usize,
}

impl<B, E, St> Cleaver<B, E, St>
    where B: Splittable + Unpin,
          St: Stream<Item=Result<B, E>>
{
    /// Construct with source and maximum size to split on.
    ///
    /// The size to split on must be at least 1.
    pub fn new(source: St, max: usize) -> Self {
        assert!(max > 0);
        Cleaver { source, rem: None, max }
    }
}

impl<B, E, St> Stream for Cleaver<B, E, St>
    where B: Splittable + Unpin,
          St: Stream<Item=Result<B, E>>
{
    type Item = Result<B, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Option<Self::Item>>
    {
        // Safety: This is for projection to src below, which is exclusively
        // owned by this wrapper and never moved. The `unsafe` could be
        // avoided, but at the cost of requiring the source stream be `Unpin`.
        let this = unsafe { self.get_unchecked_mut() };
        match this.rem {
            Some(ref mut b) => {
                match b.split_if(this.max) {
                    Some(l) => {
                        return Poll::Ready(Some(Ok(l)))
                    }
                    None => {
                        return Poll::Ready(Some(Ok(this.rem.take().unwrap())));
                    }
                }
            }
            None => {}
        }
        let src = unsafe { Pin::new_unchecked(&mut this.source) };
        match src.poll_next(cx) {
            Poll::Ready(Some(Ok(b))) => {
                this.rem = Some(b);
                // recurse for proper waking
                let this = unsafe { Pin::new_unchecked(this) };
                this.poll_next(cx)
            }
            other => other,
        }
    }
}

impl<B, E, St> fmt::Debug for Cleaver<B, E, St>
    where B: Splittable + Unpin,
          St: Stream<Item=Result<B, E>> + fmt::Debug
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cleaver")
            .field("source", &self.source)
            .field(
                "rem",
                if self.rem.is_none() {
                    &"None"
                } else {
                    &"Some(...)"
                }
            )
            .field("max", &self.max)
            .finish()
    }
}
