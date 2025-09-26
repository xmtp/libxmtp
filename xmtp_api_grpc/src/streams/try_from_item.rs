//! Maps a `TryStream` with a different type that implements `TryFrom<T>` for the streams item

use futures::{Stream, TryStream};
use pin_project_lite::pin_project;
use std::{
    marker::PhantomData,
    task::{ready, Poll},
};

pin_project! {
    pub struct TryFromItem<S, T> {
        #[pin] inner: S,
        _marker: PhantomData<T>,
    }
}

/// Wrap a `TryStream<T>` such that it converts its 'item' to T
pub fn try_from_stream<S, T>(s: S) -> TryFromItem<S, T> {
    TryFromItem::<S, T> {
        inner: s,
        _marker: PhantomData,
    }
}

impl<S, T> Stream for TryFromItem<S, T>
where
    S: TryStream,
    T: TryFrom<S::Ok>,
    S::Error: From<<T as TryFrom<S::Ok>>::Error>,
{
    type Item = Result<T, S::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        let item = ready!(this.inner.try_poll_next(cx));
        match item {
            Some(i) => Poll::Ready(Some(i.and_then(|i| i.try_into().map_err(S::Error::from)))),
            None => Poll::Ready(None),
        }
    }
}
