use openmls::{
    credentials::{BasicCredential, CredentialType},
    error::LibraryError,
    extensions::{
        Extension, ExtensionType, Extensions, Metadata, RequiredCapabilitiesExtension,
        UnknownExtension,
    },
    group::{
        CreateGroupContextExtProposalError, MlsGroupCreateConfig, MlsGroupJoinConfig,
        ProcessedWelcome,
    },
    messages::proposals::ProposalType,
    prelude::{
        BasicCredentialError, Capabilities, CredentialWithKey, Error as TlsCodecError, GroupId,
        MlsGroup as OpenMlsGroup, StagedWelcome, Welcome as MlsWelcome, WireFormatPolicy,
    },
};

use parking_lot::{Mutex, MutexGuard};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::{Arc, LazyLock},
};

pub static MLS_COMMIT_LOCK: LazyLock<Mutex<HashMap<Vec<u8>, Arc<Mutex<()>>>>> =
    LazyLock::new(Mutex::default);

pub struct SerialOpenMlsGroup<'a> {
    group: &'a mut OpenMlsGroup,
    lock: MutexGuard<'a, ()>,
    _mutex: Arc<Mutex<()>>,
}

impl<'a> Deref for SerialOpenMlsGroup<'a> {
    type Target = OpenMlsGroup;
    fn deref(&self) -> &Self::Target {
        &self.group
    }
}

impl<'a> DerefMut for SerialOpenMlsGroup<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.group
    }
}

pub(crate) trait OpenMlsLock {
    fn lock<'a>(&'a mut self) -> SerialOpenMlsGroup<'a>;
}

impl OpenMlsLock for OpenMlsGroup {
    fn lock<'a>(&'a mut self) -> SerialOpenMlsGroup<'a> {
        // .clone() is important here so that the outer lock gets dropped
        let mutex = MLS_COMMIT_LOCK
            .lock()
            .entry(self.group_id().to_vec())
            .or_default()
            .clone();

        // this may block
        let lock = mutex.lock();
        let lock = unsafe {
            // let the borrow checker know that this guard's mutex is going to be owned by the struct it's returning
            std::mem::transmute::<MutexGuard<'_, ()>, MutexGuard<'a, ()>>(lock)
        };

        SerialOpenMlsGroup {
            group: self,
            lock,
            _mutex: mutex,
        }
    }
}
