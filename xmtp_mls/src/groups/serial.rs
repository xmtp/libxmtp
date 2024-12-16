use openmls::prelude::MlsGroup as OpenMlsGroup;

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::{Arc, LazyLock},
};
use tokio::sync::{Mutex, OwnedMutexGuard};

type CommitLock = parking_lot::Mutex<HashMap<Vec<u8>, Arc<Mutex<()>>>>;
pub static MLS_COMMIT_LOCK: LazyLock<CommitLock> = LazyLock::new(parking_lot::Mutex::default);

pub struct SerialOpenMlsGroup<'a> {
    group: &'a mut OpenMlsGroup,
    _lock: OwnedMutexGuard<()>,
}

impl Deref for SerialOpenMlsGroup<'_> {
    type Target = OpenMlsGroup;
    fn deref(&self) -> &Self::Target {
        self.group
    }
}

impl DerefMut for SerialOpenMlsGroup<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.group
    }
}

#[allow(unused)]
pub(crate) trait OpenMlsLock {
    fn lock_blocking(&mut self) -> SerialOpenMlsGroup;
    async fn lock(&mut self) -> SerialOpenMlsGroup;
}

impl OpenMlsLock for OpenMlsGroup {
    #[allow(clippy::needless_lifetimes)]
    async fn lock<'a>(&'a mut self) -> SerialOpenMlsGroup<'a> {
        // .clone() is important here so that the outer lock gets dropped
        let mutex = MLS_COMMIT_LOCK
            .lock()
            .entry(self.group_id().to_vec())
            .or_default()
            .clone();

        // this may block
        let lock = mutex.lock_owned().await;

        SerialOpenMlsGroup {
            group: self,
            _lock: lock,
        }
    }

    #[allow(clippy::needless_lifetimes)]
    fn lock_blocking<'a>(&'a mut self) -> SerialOpenMlsGroup<'a> {
        // .clone() is important here so that the outer lock gets dropped
        let mutex = MLS_COMMIT_LOCK
            .lock()
            .entry(self.group_id().to_vec())
            .or_default()
            .clone();

        // this may block
        let lock = mutex.blocking_lock_owned();

        SerialOpenMlsGroup {
            group: self,
            _lock: lock,
        }
    }
}
