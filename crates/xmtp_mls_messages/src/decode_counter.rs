//! Test-only count at the real standard text decoder.

use std::sync::Mutex;

static WATCHED: Mutex<Option<(Vec<u8>, u64)>> = Mutex::new(None);

/// Start a count for one unique text payload. Other messages do not affect it.
pub fn watch(text: String) {
    *WATCHED.lock().expect("decode counter lock") = Some((text.into_bytes(), 0));
}

pub(super) fn record(content: &[u8]) {
    if let Some((watched, count)) = WATCHED.lock().expect("decode counter lock").as_mut()
        && content == watched
    {
        *count += 1;
    }
}

/// Read the number of matching decoder calls since `watch`.
pub fn count() -> u64 {
    WATCHED
        .lock()
        .expect("decode counter lock")
        .as_ref()
        .map_or(0, |(_, count)| *count)
}
