use crate::app::types::InboxId;
use color_eyre::eyre::{self, eyre};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Mutex as TokioMutex;

type IdentityLockMap = Arc<Mutex<HashMap<InboxId, Arc<TokioMutex<()>>>>>;

static IDENTITY_LOCKS: OnceLock<IdentityLockMap> = OnceLock::new();

pub fn get_identity_lock(inbox_id: &InboxId) -> Result<Arc<TokioMutex<()>>, eyre::Error> {
    let locks = IDENTITY_LOCKS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())));
    let mut map = locks
        .lock()
        .map_err(|e| eyre!("Failed to lock IDENTITY_LOCKS: {}", e))?;
    Ok(map
        .entry(*inbox_id)
        .or_insert_with(|| Arc::new(TokioMutex::new(())))
        .clone())
}
