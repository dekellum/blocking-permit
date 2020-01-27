use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::stream::Stream;

/// A `Stream` adapter that yields after every Ready result from its source.
///
/// This may be useful to ensure that a Future that polls a Stream (directly or
/// indirectly) yields periodically.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct YieldStream<St, I>
    where St: Stream<Item=I>
{
    source: St,
    yielded: bool,
}

impl<St, I> YieldStream<St, I>
    where St: Stream<Item=I>
{
    /// Construct with source and maximum size to split on.
    ///
    /// The size to split on must be at least 1.
    pub fn new(source: St) -> Self {
        YieldStream { source, yielded: true }
    }
}

impl<St, I> Stream for YieldStream<St, I>
    where St: Stream<Item=I>
{
    type Item = I;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Option<Self::Item>>
    {
        // Safety: This is for projection to src below, which is exclusively
        // owned by this wrapper and never moved. The `unsafe` could be
        // avoided, but at the cost of requiring the source stream be `Unpin`.
        let this = unsafe { self.get_unchecked_mut() };

        if this.yielded {
            let src = unsafe { Pin::new_unchecked(&mut this.source) };
            let next = src.poll_next(cx);
            if let Poll::Ready(Some(_)) = next {
                this.yielded = false;
            }
            next
        } else {
            this.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
