//! Multiplexed Stream Type

use std::{pin::Pin, task::Poll};

use futures::{Stream, stream::FusedStream};
use pin_project::pin_project;
use std::task::Context;

/// Attempts to pull items from both streams. S1 will always
/// be polled before S2. if S1 finishes first, the stream is considered over.
/// Attempts naive fairness by polling S2 before stream close if S1 finished and S2 still has Ready
/// items.
pub fn multiplexed<S1, S2>(s1: S1, s2: S2) -> MultiplexedStream<S1, S2> {
    MultiplexedStream {
        s1,
        s2,
        s1_ended: false,
        terminated: false,
    }
}

#[pin_project]
/// Stream for the [multiplexed()] function
pub struct MultiplexedStream<S1, S2> {
    #[pin]
    s1: S1,
    #[pin]
    s2: S2,
    s1_ended: bool,
    terminated: bool,
}

impl<S1, S2> MultiplexedStream<S1, S2>
where
    S1: Stream<Item = S2::Item>,
    S2: Stream,
{
    fn poll_s2(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<S2::Item>> {
        let mut this = self.as_mut().project();
        if let Poll::Ready(Some(item)) = this.s2.as_mut().poll_next(cx) {
            return Poll::Ready(Some(item));
        }
        *this.terminated = true;
        Poll::Ready(None)
    }
}

impl<S1, S2> Stream for MultiplexedStream<S1, S2>
where
    S1: Stream<Item = S2::Item>,
    S2: Stream,
{
    type Item = S2::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();
        if *this.terminated {
            return Poll::Ready(None);
        }
        if *this.s1_ended {
            return self.poll_s2(cx);
        }
        match this.s1.as_mut().poll_next(cx) {
            Poll::Ready(Some(item)) => {
                return Poll::Ready(Some(item));
            }
            Poll::Ready(None) => {
                *this.s1_ended = true;
                return self.poll_s2(cx);
            }
            Poll::Pending => (),
        };

        match this.s2.as_mut().poll_next(cx) {
            Poll::Ready(Some(item)) => Poll::Ready(Some(item)),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S1, S2> FusedStream for MultiplexedStream<S1, S2>
where
    S1: Stream<Item = S2::Item>,
    S2: Stream,
{
    fn is_terminated(&self) -> bool {
        self.terminated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use futures_test::{
        assert_stream_done, assert_stream_next, stream::StreamTestExt, task::noop_context,
    };

    #[xmtp_common::test]
    fn does_not_starve_s2() {
        let s1 = stream::iter(vec![1, 2, 3]);
        let s2 = stream::iter(vec![4, 5, 6]);
        let stream = multiplexed(s1, s2);
        futures::pin_mut!(stream);
        for i in 1..=6 {
            assert_stream_next!(stream, i);
        }
        assert_stream_done!(stream)
    }

    #[xmtp_common::test]
    fn polls_s2_in_between_s1() {
        let s1 = stream::iter(vec![1, 2, 3]).interleave_pending();
        let s2 = stream::iter(vec![4, 5, 6]);
        let stream = multiplexed(s1, s2);
        futures::pin_mut!(stream);
        assert_stream_next!(stream, 4);
        assert_stream_next!(stream, 1);
        assert_stream_next!(stream, 5);
        assert_stream_next!(stream, 2);
        assert_stream_next!(stream, 6);
        assert_stream_next!(stream, 3);
        assert_stream_done!(stream)
    }

    #[xmtp_common::test]
    fn ignores_items_after_s2_pending() {
        let s1 = stream::iter(vec![1]);
        let s2 = stream::iter(vec![4, 5, 6]).interleave_pending();
        let stream = multiplexed(s1, s2);
        futures::pin_mut!(stream);
        assert_stream_next!(stream, 1);
        assert_stream_done!(stream)
    }

    #[xmtp_common::test]
    fn ends_when_s1_ends() {
        let s1 = stream::iter(vec![1, 2, 3]);
        let s2 = stream::iter(vec![]); // s2 ends immediately, but s1 should keep going
        let stream = multiplexed(s1, s2);
        futures::pin_mut!(stream);
        for i in 1..=3 {
            assert_stream_next!(stream, i);
        }
        assert_stream_done!(stream)
    }

    #[xmtp_common::test]
    fn does_not_panic_on_polling_after_finish() {
        let s1 = stream::iter(vec![1]);
        let s2 = stream::iter(vec![]);
        let stream = multiplexed(s1, s2);
        futures::pin_mut!(stream);
        assert_stream_next!(stream, 1);
        assert_stream_done!(stream);
        assert!(stream.is_terminated());
        let mut cx = noop_context();
        let res = stream.as_mut().poll_next(&mut cx);
        assert_eq!(res, Poll::Ready(None));
        let res = stream.as_mut().poll_next(&mut cx);
        assert_eq!(res, Poll::Ready(None));
    }
}
