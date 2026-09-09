//! Convert collections and keep each conversion error in the stream.
use futures::{Stream, StreamExt, stream};

pub fn try_extractor<S, T, U, E, F>(stream: S, decode: F) -> impl Stream<Item = Result<U, E>>
where
    S: Stream<Item = Result<Vec<T>, E>>,
    F: Fn(T) -> Result<U, E> + Clone,
{
    stream
        .map(move |result| {
            let items = match result {
                Ok(items) => items.into_iter().map(decode.clone()).collect(),
                Err(error) => vec![Err(error)],
            };
            stream::iter(items)
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[xmtp_common::test(unwrap_try = true)]
    async fn preserves_order_and_all_errors() {
        let input = stream::iter(vec![Ok(vec![]), Ok(vec![1, 2, 3]), Err("wire")]);
        let output: Vec<_> = try_extractor(input, |n| if n == 2 { Err("decode") } else { Ok(n) })
            .collect()
            .await;
        assert_eq!(output, vec![Ok(1), Err("decode"), Ok(3), Err("wire")]);
    }
    #[xmtp_common::test(unwrap_try = true)]
    async fn empty_stream_finishes() {
        let output: Vec<_> = try_extractor(stream::empty::<Result<Vec<u32>, ()>>(), Ok)
            .collect()
            .await;
        assert!(output.is_empty());
    }
}
