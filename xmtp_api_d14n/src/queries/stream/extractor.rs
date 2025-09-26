//! Extracts & flattens items from a `TryStream` whose items implement [`EnvelopeCollection`] with extractor ('T')

use futures::{Stream, TryStream};
use pin_project_lite::pin_project;
use std::{
    collections::VecDeque,
    marker::PhantomData,
    task::{Poll, ready},
};

use crate::protocol::{
    EnvelopeCollection, EnvelopeError, EnvelopeVisitor, TryEnvelopeCollectionExt, TryExtractor,
};

pin_project! {
    pub struct TryExtractorStream<S, E: TryExtractor> {
        #[pin] inner: S,
        buffered: VecDeque<<E as TryExtractor>::Ok>,
        _marker: PhantomData<E>,
    }
}

/// Wrap a `TryStream<T>` such that it converts its 'item' to T
// _NOTE_: extractor accepted as argument to avoid a requirement on
// specifying `Stream` type
pub fn try_extractor<S, E>(s: S) -> TryExtractorStream<S, E>
where
    E: TryExtractor,
{
    TryExtractorStream::<S, E> {
        inner: s,
        buffered: Default::default(),
        _marker: PhantomData,
    }
}

impl<S, E> Stream for TryExtractorStream<S, E>
where
    S: TryStream,
    for<'a> S::Ok: EnvelopeCollection<'a> + std::fmt::Debug,
    for<'a> S::Error: From<EnvelopeError>,
    for<'a> E: TryExtractor + EnvelopeVisitor<'a> + Default,
    for<'a> EnvelopeError:
        From<<E as EnvelopeVisitor<'a>>::Error> + From<<E as TryExtractor>::Error>,
{
    type Item = Result<E::Ok, S::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        if let Some(item) = this.buffered.pop_front() {
            return Poll::Ready(Some(Ok(item)));
        }
        let envelope = ready!(this.inner.try_poll_next(cx));
        match envelope {
            Some(item) => {
                let consumed: Vec<E::Ok> = item?.try_consume::<E>()?;
                this.buffered.extend(consumed);
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            None => Poll::Ready(None),
        }
    }
}
