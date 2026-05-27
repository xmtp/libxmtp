use std::{collections::HashMap, sync::Arc};

use tokio::sync::Mutex;
use xmtp_proto::types::GroupId;

/// A registry of mutexes that can be locked and unlocked by a given key.
#[derive(Debug, Clone, Default)]
pub struct MutexRegistry {
    mutexes: HashMap<GroupId, Arc<Mutex<()>>>,
}

impl MutexRegistry {
    pub fn new() -> Self {
        Self {
            mutexes: HashMap::new(),
        }
    }

    pub fn get_mutex(&mut self, key: GroupId) -> Arc<Mutex<()>> {
        self.mutexes
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}
