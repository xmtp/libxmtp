//! Synchronous access to mutable MLS state under the database writer.

use openmls::group::MlsGroup as OpenMlsGroup;
use xmtp_db::{
    NotFound, StorageError, TransactionOutcome, TransactionalKeyStore, XmtpMlsStorageProvider,
    sql_key_store::SqlKeyStoreError,
};
use xmtp_proto::types::GroupId;

/// A capability created only after an immediate write transaction starts.
///
/// Keep network requests and callbacks outside this capability. A group is
/// loaded for one synchronous operation and is dropped before the transaction
/// ends. A retry must enter a new transaction and load the group again.
pub(crate) struct StateTx<'a, T> {
    query: &'a mut T,
}

impl<T: TransactionalKeyStore> StateTx<'_, T> {
    /// Load current state after the database writer has been acquired.
    ///
    /// The operation borrows the group. It cannot return the mutable group or
    /// a future that borrows it. All storage access uses this transaction.
    pub(crate) fn with_group<R, E>(
        &mut self,
        group_id: GroupId,
        operation: impl FnOnce(&mut OpenMlsGroup, &T::Store<'_>) -> Result<R, E>,
    ) -> Result<R, E>
    where
        E: From<StorageError> + From<SqlKeyStoreError>,
    {
        let storage = self.query.key_store();
        let mut group = OpenMlsGroup::load(&storage, &group_id.to_openmls())?
            .ok_or_else(|| StorageError::from(NotFound::MlsGroup(group_id)))?;
        operation(&mut group, &storage)
    }

    /// Access keys and records for a state operation that does not load a group.
    pub(crate) fn storage(&mut self) -> T::Store<'_> {
        self.query.key_store()
    }

    /// Roll back a failed attempt while retaining the outer writer lock.
    ///
    /// The failed attempt's group is dropped before the caller can load state
    /// to record a rejection. This also discards OpenMLS ratchet caches.
    pub(crate) fn savepoint<R, E>(
        &mut self,
        operation: impl FnOnce(
            &mut StateTx<'_, <T::Store<'_> as XmtpMlsStorageProvider>::TxQuery>,
        ) -> Result<TransactionOutcome<R>, E>,
    ) -> Result<TransactionOutcome<R>, E>
    where
        E: From<xmtp_db::diesel::result::Error>
            + From<xmtp_db::ConnectionError>
            + std::error::Error,
    {
        self.query
            .key_store()
            .savepoint(|query| operation(&mut StateTx { query }))
    }
}

/// Acquire the cross-process writer before exposing state or keys.
///
/// An error rolls back all writes. An explicit rollback is useful for trials;
/// the caller must keep only immutable requirements from such a trial.
pub(crate) fn state_write<S, R, E>(
    storage: &S,
    operation: impl FnOnce(&mut StateTx<'_, S::TxQuery>) -> Result<TransactionOutcome<R>, E>,
) -> Result<TransactionOutcome<R>, E>
where
    S: XmtpMlsStorageProvider,
    E: From<xmtp_db::diesel::result::Error> + From<xmtp_db::ConnectionError> + std::error::Error,
{
    storage.transaction(|query| {
        let outcome = operation(&mut StateTx { query })?;
        #[cfg(all(test, not(target_arch = "wasm32")))]
        if matches!(outcome, TransactionOutcome::Continue(_)) && precommit_test_hook::is_set() {
            use xmtp_db::ConnectionExt;
            query.key_store().db().raw_query(|conn| {
                precommit_test_hook::run(conn);
                Ok(())
            })?;
        }
        Ok(outcome)
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) mod precommit_test_hook {
    use std::{cell::RefCell, marker::PhantomData, rc::Rc};
    use xmtp_db::diesel::SqliteConnection;

    type Hook = Box<dyn FnMut(&mut SqliteConnection)>;

    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    /// Keep the hook on the thread that owns the synchronous state operation.
    pub(crate) struct Guard {
        previous: Option<Hook>,
        _same_thread: PhantomData<Rc<()>>,
    }

    pub(crate) fn install(hook: impl FnMut(&mut SqliteConnection) + 'static) -> Guard {
        Guard {
            previous: HOOK.with(|slot| slot.replace(Some(Box::new(hook)))),
            _same_thread: PhantomData,
        }
    }

    pub(super) fn is_set() -> bool {
        HOOK.with(|slot| slot.borrow().is_some())
    }

    pub(super) fn run(conn: &mut SqliteConnection) {
        HOOK.with(|slot| {
            if let Some(hook) = slot.borrow_mut().as_mut() {
                hook(conn);
            }
        });
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            HOOK.with(|slot| {
                slot.replace(self.previous.take());
            });
        }
    }
}
