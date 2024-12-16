use openmls::prelude::MlsGroup as OpenMlsGroup;

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::{Arc, LazyLock},
};
use tokio::sync::{Mutex, MutexGuard};

type CommitLock = parking_lot::Mutex<HashMap<Vec<u8>, Arc<Mutex<()>>>>;
pub static MLS_COMMIT_LOCK: LazyLock<CommitLock> = LazyLock::new(parking_lot::Mutex::default);

pub struct SerialOpenMlsGroup<'a> {
    group: &'a mut OpenMlsGroup,
    _lock: MutexGuard<'a, ()>,
    _mutex: Arc<Mutex<()>>,
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
        let lock = mutex.lock().await;
        let lock = unsafe {
            // let the borrow checker know that this guard's mutex is going to be owned by the struct it's returning
            std::mem::transmute::<MutexGuard<'_, ()>, MutexGuard<'a, ()>>(lock)
        };

        SerialOpenMlsGroup {
            group: self,
            _lock: lock,
            _mutex: mutex,
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
        let lock = mutex.blocking_lock();
        let lock = unsafe {
            // let the borrow checker know that this guard's mutex is going to be owned by the struct it's returning
            std::mem::transmute::<MutexGuard<'_, ()>, MutexGuard<'a, ()>>(lock)
        };

        SerialOpenMlsGroup {
            group: self,
            _lock: lock,
            _mutex: mutex,
        }
    }
}
