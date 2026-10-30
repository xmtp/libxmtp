//! Bounded window fan-out. No envelope payload is selected here.

use std::collections::HashMap;

use super::channel::{Delivery, DeliveryConfig};
use crate::{db::Store, error::Error};
use xmtp_cryptography::secret::Zeroizing;

pub(crate) const PAGE_SIZE: usize = 1000;
pub(crate) const WINDOW_ROWS: i64 = 1024;

struct Row {
    first_id: Option<i64>,
    last_id: Option<i64>,
    sequence_id: Option<i64>,
    topic: Option<Vec<u8>>,
    server_ns: Option<i64>,
    sender_hmac: Option<Vec<u8>>,
    recipient_id: Option<Vec<u8>>,
    secret_hash: Option<Vec<u8>>,
    hmac_epoch_base: Option<i64>,
    hmac_key_0: Option<Vec<u8>>,
    hmac_key_1: Option<Vec<u8>>,
    hmac_key_2: Option<Vec<u8>>,
    channel: Option<i16>,
    delivery: Option<String>,
    signing_key: Option<Vec<u8>>,
}

pub(crate) struct Page {
    pub first: Option<i64>,
    pub last: Option<i64>,
    pub next: (i64, Vec<u8>),
    pub full: bool,
    pub deliveries: Vec<Delivery>,
    pub suppressed: Vec<crate::db::PushChannel>,
}

/// Read one page and suppress only matching senders. The cache belongs to the
/// current row, including all of its pages, and holds its decoded data and
/// remaining check budget. Ordered fan-out needs at most one cached entry.
/// A database failure loads no work.
// implements: PUSH-230
#[xmtp_common::db_span]
pub(crate) async fn load(
    store: &Store,
    position: i64,
    boundary: i64,
    after: &(i64, Vec<u8>),
    payloads: &mut HashMap<i64, (Option<Vec<u8>>, usize)>,
) -> Result<Page, Error> {
    let rows = sqlx::query_file_as!(
        Row,
        "src/push/window.sql",
        position,
        boundary,
        WINDOW_ROWS,
        after.0,
        &after.1,
        PAGE_SIZE as i64
    )
    .fetch_all(&store.read)
    .await?;
    let mut page = Page {
        first: rows.first().and_then(|row| row.first_id),
        last: rows.first().and_then(|row| row.last_id),
        next: after.clone(),
        full: false,
        deliveries: Vec::new(),
        suppressed: Vec::new(),
    };
    let mut count = 0;
    for row in rows {
        let Some(sequence_id) = row.sequence_id else {
            continue;
        };
        let missing = || Error::Invariant("incomplete push fan-out row");
        let recipient_id = row.recipient_id.ok_or_else(missing)?;
        page.next = (sequence_id, recipient_id.clone());
        count += 1;
        let channel = row.channel.ok_or_else(missing)?.try_into()?;
        let keys =
            [row.hmac_key_0, row.hmac_key_1, row.hmac_key_2].map(|key| key.map(Zeroizing::new));
        // Wrap before the suppression check, so a suppressed delivery wipes it too.
        let signing_key = row.signing_key.map(Zeroizing::new);
        if let Some(key) = super::suppress::key(
            row.server_ns.ok_or_else(missing)?,
            row.hmac_epoch_base,
            &keys,
        )
        .filter(|_| row.sender_hmac.is_some())
        {
            if !payloads.contains_key(&sequence_id) {
                // Sequence ids increase across pages. Release the prior payload
                // before loading the next one; no later delivery needs it.
                payloads.clear();
                let payload: Option<Vec<u8>> = sqlx::query_scalar!(
                    "SELECT payload FROM envelopes WHERE sequence_id = $1",
                    sequence_id
                )
                .fetch_optional(&store.read)
                .await?;
                let data = payload.as_deref().and_then(super::suppress::data);
                payloads.insert(sequence_id, (data, super::suppress::BUDGET_BYTES));
            }
            let (data, budget) = payloads.get_mut(&sequence_id).ok_or_else(missing)?;
            // Past the budget, deliver unchecked.
            if let Some(data) = data.as_deref().filter(|data| data.len() <= *budget) {
                *budget -= data.len();
                if super::suppress::matches(
                    data,
                    key,
                    row.sender_hmac.as_deref().unwrap_or_default(),
                ) {
                    page.suppressed.push(channel);
                    continue;
                }
            }
        }
        page.deliveries.push(Delivery {
            payload: xmtp_push_types::PushPayload::new(
                &row.topic.ok_or_else(missing)?,
                sequence_id as u64,
            ),
            config: DeliveryConfig {
                recipient_id,
                secret_hash: row.secret_hash.ok_or_else(missing)?,
                channel,
                delivery: row.delivery.ok_or_else(missing)?,
                signing_key,
            },
        });
    }
    page.full = count == PAGE_SIZE;
    Ok(page)
}
