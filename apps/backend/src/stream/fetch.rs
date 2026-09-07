use std::collections::HashMap;
use std::sync::Arc;
use tonic::Status;
use crate::db::{self, StoredEnvelope, stream::Range};
use super::{StreamHub, FETCH_ROWS, ENVELOPE_OVERHEAD, output::Reservation};

pub(super) struct Request { pub range: Range, pub generation: u64 }
pub(super) struct ResultPage {
    pub requests: Vec<Request>,
    pub rows: Vec<(usize, StoredEnvelope)>,
    pub visited: usize,
    pub reservation: Reservation,
}

/// Fetch one fair turn under a shared concurrency permit. Reserve output bytes
/// before reading payloads, preserve topic prefixes at a byte cutoff, and release
/// the snapshot before returning. Dropping the future cancels its outstanding work.
pub(super) async fn fetch(hub: Arc<StreamHub>, requests: Vec<Request>, budget: usize, reservation: Reservation) -> Result<ResultPage, Status> {
    let _slot = hub.fetches.clone().acquire_owned().await.map_err(|_| Status::unavailable("fetch service stopped"))?;
    let ranges: Vec<_> = requests.iter().map(|request| request.range.clone()).collect();
    let mut tx = db::stream::snapshot(&hub.read).await.map_err(|_| Status::unavailable("history database unavailable"))?;
    let candidates = db::stream::history(&mut tx, &ranges, FETCH_ROWS).await.map_err(|_| Status::unavailable("history read failed"))?;
    let mut bytes = 0;
    let mut selected = HashMap::new();
    let mut visited = requests.len();
    for (ordinal, candidate) in candidates {
        if candidate.topic != requests[ordinal].range.topic { return Err(Status::unavailable("history topic mismatch")); }
        let size = candidate.payload_bytes as usize + ENVELOPE_OVERHEAD;
        if bytes + size > budget {
            if selected.is_empty() { return Err(Status::resource_exhausted("stored envelope exceeds delivery capacity")); }
            visited = ordinal; if selected.values().any(|&value| value == ordinal) { visited += 1; } break;
        }
        bytes += size;
        selected.insert(candidate.sequence_id, ordinal);
    }
    let ids: Vec<_> = selected.keys().copied().collect();
    for ordinal in 0..visited {
        if !selected.values().any(|&value| value == ordinal) { return Err(Status::unavailable("visible topic head has no retained envelope")); }
    }
    let rows = db::stream::payloads(&mut tx, &ids).await.map_err(|_| Status::unavailable("history payload read failed"))?;
    if rows.len() != ids.len() { return Err(Status::unavailable("history candidate disappeared")); }
    tx.commit().await.map_err(|_| Status::unavailable("history snapshot failed"))?;
    let mut rows: Vec<_> = rows.into_iter().map(|row| (selected[&row.sequence_id], row)).collect();
    rows.sort_by_key(|(ordinal, row)| (*ordinal, row.sequence_id));
    Ok(ResultPage { requests, rows, visited, reservation })
}
