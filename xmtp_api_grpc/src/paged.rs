use std::future::Future;

use xmtp_common::{retry::Strategy as RetryStrategy, retry_async, Retry, RetryableError};
use xmtp_configuration::MAX_PAGE_SIZE;
use xmtp_proto::api_client::Paged;

pub async fn retryable_paged_request<F, Fut, T, E, S>(
    retry: &Retry<S>,
    sequence_id: Option<u64>,
    f: F,
) -> Result<Vec<<T as Paged>::Message>, E>
where
    F: Fn(Option<u64>) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: RetryableError,
    T: Paged,
    S: RetryStrategy,
{
    let mut out: Vec<<T as Paged>::Message> = vec![];

    let mut id_cursor = sequence_id;
    loop {
        let result = retry_async!(retry, (async { f(id_cursor).await }))?;
        let info = result.info().clone();
        let mut messages = result.messages();
        let num_messages = messages.len();
        out.append(&mut messages);

        if num_messages < MAX_PAGE_SIZE as usize || info.is_none() {
            break;
        }

        let paging_info = info.expect("Empty paging info");
        if paging_info.id_cursor == 0 {
            break;
        }

        id_cursor = Some(paging_info.id_cursor);
    }
    Ok(out)
}
