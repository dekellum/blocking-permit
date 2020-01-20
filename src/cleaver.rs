use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::stream::Stream;

/// Trait for buffer types that may be split at a maximum length.
pub trait Splittable: Sized {
    /// If self has length greater then the specific maximum, split it into
    /// two, returning the new leading segment and with self mutated to
    /// contain the remainder.
    fn split_if(&mut self, max: usize) -> Option<Self>;
}

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
pub struct Cleaver<B, E, St>
    where B: Splittable + Unpin,
          St: Stream<Item=Result<B, E>>
{
    source: St,
    rem: Option<B>,
    max: usize,
}

impl<B, E, St> Stream for Cleaver<B, E, St>
    where B: Splittable + Unpin,
          St: Stream<Item=Result<B, E>>
{
    type Item = Result<B, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Option<Self::Item>>
    {
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
        f.debug_struct("File")
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
