use futures::stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::task::JoinHandle;

/// BufferedStream wraps an existing stream and buffers all messages in memory
/// to prevent backpressure to the underlying stream.
pub struct BufferedStream<T, E> {
    receiver: UnboundedReceiver<Result<T, E>>,
    _handle: JoinHandle<()>,
}

impl<T, E> BufferedStream<T, E>
where
    T: Send + 'static,
    E: Send + 'static,
{
    /// Creates a new BufferedStream from an existing stream
    pub fn new<S>(mut stream: S) -> Self
    where
        S: Stream<Item = Result<T, E>> + Send + Unpin + 'static,
    {
        let (sender, receiver) = unbounded_channel();

        let handle = tokio::task::spawn(async move {
            use futures::StreamExt;

            while let Some(item) = stream.next().await {
                if sender.send(item).is_err() {
                    // Receiver dropped, stop reading
                    break;
                }
            }
        });

        Self {
            receiver,
            _handle: handle,
        }
    }
}

impl<T, E> Stream for BufferedStream<T, E> {
    type Item = Result<T, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

impl<T, E> Unpin for BufferedStream<T, E> {}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_basic_buffering() {
        let messages: Vec<Result<u64, &str>> = vec![
            Ok(1),
            Ok(2),
            Ok(3),
        ];

        let stream = stream::iter(messages);
        let mut buffered = BufferedStream::new(stream);

        let result1 = buffered.next().await.unwrap().unwrap();
        assert_eq!(result1, 1);

        let result2 = buffered.next().await.unwrap().unwrap();
        assert_eq!(result2, 2);

        let result3 = buffered.next().await.unwrap().unwrap();
        assert_eq!(result3, 3);

        let result4 = buffered.next().await;
        assert!(result4.is_none());
    }

    #[tokio::test]
    async fn test_fast_producer_slow_consumer() {
        let message_count = 100usize;
        let messages: Vec<Result<usize, &str>> = (0..message_count).map(Ok).collect();

        let stream = stream::iter(messages);
        let mut buffered = BufferedStream::new(stream);

        sleep(Duration::from_millis(100)).await;

        let mut received = Vec::new();
        while let Some(msg) = buffered.next().await {
            received.push(msg.unwrap());
        }

        assert_eq!(received.len(), message_count);
        assert_eq!(received, (0..message_count).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn test_error_propagation() {
        let error_msg = "test error";
        let messages: Vec<Result<u64, &str>> = vec![
            Ok(1),
            Ok(2),
            Err(error_msg),
            Ok(3),
        ];

        let stream = stream::iter(messages);
        let mut buffered = BufferedStream::new(stream);

        assert!(buffered.next().await.unwrap().is_ok());
        assert!(buffered.next().await.unwrap().is_ok());
        
        let error_result = buffered.next().await.unwrap();
        assert!(error_result.is_err());
        
        assert!(buffered.next().await.unwrap().is_ok());
        assert!(buffered.next().await.is_none());
    }

    #[tokio::test]
    async fn test_stream_completion() {
        let messages: Vec<Result<u64, &str>> = vec![Ok(1), Ok(2), Ok(3)];
        let stream = stream::iter(messages);
        let mut buffered = BufferedStream::new(stream);

        let mut count = 0;
        while buffered.next().await.is_some() {
            count += 1;
        }
        assert_eq!(count, 3);

        assert!(buffered.next().await.is_none());
        assert!(buffered.next().await.is_none());
    }

    #[tokio::test]
    async fn test_drop_behavior() {
        let messages: Vec<Result<u64, &str>> = (0..1000).map(Ok).collect();
        let stream = stream::iter(messages);
        let mut buffered = BufferedStream::new(stream);

        let _ = buffered.next().await;
        let _ = buffered.next().await;

        drop(buffered);
    }

    #[tokio::test]
    async fn test_slow_stream_no_blocking() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU64, Ordering};

        let produced = Arc::new(AtomicU64::new(0));
        let produced_clone = produced.clone();

        let stream = Box::pin(stream::unfold(0u64, move |state| {
            let produced = produced_clone.clone();
            async move {
                if state < 10 {
                    sleep(Duration::from_millis(10)).await;
                    produced.fetch_add(1, Ordering::SeqCst);
                    Some((Ok::<u64, &str>(state), state + 1))
                } else {
                    None
                }
            }
        }));

        let mut buffered = BufferedStream::new(stream);

        sleep(Duration::from_millis(150)).await;

        let produced_count = produced.load(Ordering::SeqCst);
        assert!(produced_count >= 5, "Should have produced at least 5 messages, got {}", produced_count);

        let mut consumed = 0;
        while buffered.next().await.is_some() {
            consumed += 1;
        }
        assert_eq!(consumed, 10);
    }
}