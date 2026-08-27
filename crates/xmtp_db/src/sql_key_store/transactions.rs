use super::*;
#[cfg(feature = "sqlite")]
use crate::DbConnection;
use crate::TransactionOutcome;
use crate::TransactionOutcome::{Continue, Rollback};

/// Resolve a transaction body that the SQLite backend can only run synchronously.
///
/// The `Query*` traits are async on both backends, but diesel drives a transaction
/// through `&mut SqliteConnection` inside a closure that cannot be async. On the
/// SQLite backend that is reconcilable: those futures are await-free -- every body is
/// a blocking diesel call -- so one poll always completes them. This is the single
/// place that assumption is cashed in; keeping it here rather than at each call
/// site means a body that ever does yield fails loudly in one known spot.
pub(crate) fn drive_to_completion<F: std::future::Future>(fut: F) -> F::Output {
    use std::task::{Context, Poll, Waker};
    // Poll exactly once with a no-op waker (what `FutureExt::now_or_never` does).
    // Done with std rather than the `futures` crate so this core path carries no
    // dependency of its own — `futures` is otherwise only a test-utils dep here.
    let mut fut = std::pin::pin!(fut);
    match fut.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        // A single poll is enough because on the SQLite (diesel) backend every
        // storage op is synchronous under an `async fn` — i.e. an already-ready future that
        // completes in one poll. `Pending` therefore means the transaction body
        // awaited something that actually SUSPENDS (a network call, a timer, a real
        // async lock). That is forbidden here: this runs inside `immediate_
        // transaction`, so suspending would hold the SQLite write lock across the
        // await. We panic loudly rather than rely on reviewer discipline — the same
        // mistake in C/C++ just deadlocks silently. Move the awaited work OUTSIDE
        // the transaction. (The Postgres backend drives these bodies with a real
        // executor and never takes this path.)
        Poll::Pending => {
            panic!(
                "a SQLite transaction body awaited an operation that \
                 suspended (returned Pending) — almost certainly a network call. \
                 Storage transactions on the SQLite backend must complete synchronously; \
                 move the awaited work outside the transaction."
            )
        }
    }
}

/// wrapper around a mutable connection (&mut SqliteConnection)
/// Requires that all execution/transaction happens in one thread on one connection.
/// This connection _must only_ be created from starting a transaction
pub struct MutableTransactionConnection<'a> {
    // we cannot avoid interior mutability here
    // because raw_query methods require &self, as do MlsStorage trait methods.
    // Since transactions are not async, once a transaction is started
    // we can ensure it occurs all on one thread.
    pub(crate) conn: parking_lot::Mutex<&'a mut SqliteConnection>,
}

impl<'a> MutableTransactionConnection<'a> {
    pub fn new(conn: &'a mut SqliteConnection) -> Self {
        Self {
            conn: parking_lot::Mutex::new(conn),
        }
    }
}

#[cfg(feature = "sqlite")]
impl<'a> ConnectionExt for MutableTransactionConnection<'a> {
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.try_lock().expect("Lock is held somewhere else");
        fun(&mut conn).map_err(crate::ConnectionError::from)
    }

    // this should cause a transaction rollback. since reconnect/disconnect is retryable
    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        Err(crate::ConnectionError::DisconnectInTransaction)
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        Err(crate::ConnectionError::ReconnectInTransaction)
    }
}

/// `SqlKeyStore` is the diesel/SQLite openmls provider, so it can only satisfy
/// `XmtpMlsStorageProvider` on the SQLite backend — its `DbQuery<'a>` associated type
/// names `DbConnection`, whose `Query*` impls only exist there. Servers use
/// `openmls_sqlx_storage` instead.
#[cfg(feature = "sqlite")]
impl<C: ConnectionExt> XmtpMlsStorageProvider for SqlKeyStore<C> {
    type Connection = C;

    type TxQuery = SqliteConnection;

    type DbQuery<'a>
        = DbConnection<&'a C>
    where
        Self::Connection: 'a;

    fn db<'a>(&'a self) -> Self::DbQuery<'a> {
        DbConnection::new(&self.conn)
    }

    // A bare `fn` returning `impl Future + MaybeSend` (not `async fn`) so the future
    // is `Send` for spawned stream/worker tasks — async-fn-in-trait doesn't imply
    // Send. The body is synchronous (drives `f` to completion), wrapped in a ready
    // `async move`. TODO: re-add `db_span` instrumentation inside the async block.
    #[allow(clippy::manual_async_fn)]
    fn transaction<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + xmtp_common::MaybeSend
    where
        T: xmtp_common::MaybeSend,
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>
            + xmtp_common::MaybeSend,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        async move {
            let conn = &self.conn;

            // immediate transactions force SQLite to respect BUSY_TIMEOUT
            // there are a few ways we can get DB Locked Errors:
            // 1.) A Transaction is already writing
            //  https://www.sqlite.org/rescode.html#busy
            // 2.) Promoting a transaction to write:
            // we start a transaction with BEGIN (read), then later promote the transaction to a write.
            // another transaction is already writing, so SQLite throws Database Locked.
            // code: https://www.sqlite.org/rescode.html#busy_snapshot
            // Solution:
            // - set BUSY_TIMEOUT. this is effectively a timeout for SQLite to get a lock on the
            //      write to a table. See [BUSY_TIMEOUT](xmtp_db::configuration::BUSY_TIMEOUT)
            // - use immediate_transaction to force SQLite to respect busy_timeout as soon as the
            //      transaction starts. Otherwise, we still run into problem #2, even if BUSY_TIMEOUT is
            //      set.

            // An intentional `Rollback` is turned into diesel's `RollbackTransaction`
            // sentinel (the only way to make diesel roll back) and flagged, so below we
            // can report it as `Ok(Rollback)` without inspecting the opaque `E`; any
            // other `Err` is a real failure and propagates.
            let mut rolled_back = false;
            let inner_result: Result<TransactionOutcome<T>, E> = conn
                .raw_query(|c| {
                    Ok(
                        c.immediate_transaction(|sqlite_c| {
                            match drive_to_completion(f(sqlite_c)) {
                                Ok(Continue(v)) => Ok(Continue(v)),
                                Ok(Rollback) => {
                                    rolled_back = true;
                                    Err(E::from(diesel::result::Error::RollbackTransaction))
                                }
                                Err(e) => Err(e),
                            }
                        }),
                    )
                })
                .map_err(E::from)?;

            // A failed ROLLBACK after our sentinel is still reported as Ok(Rollback);
            // that rare rollback-execution error is deliberately swallowed.
            match inner_result {
                Ok(outcome) => Ok(outcome),
                Err(_) if rolled_back => Ok(Rollback),
                Err(e) => Err(e),
            }
        }
    }

    // Same Rollback-sentinel handling as `transaction`; see there for the rationale.
    #[allow(clippy::manual_async_fn)]
    fn savepoint<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + xmtp_common::MaybeSend
    where
        T: xmtp_common::MaybeSend,
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>
            + xmtp_common::MaybeSend,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        async move {
            let mut rolled_back = false;
            let inner_result: Result<TransactionOutcome<T>, E> = self
                .conn
                .raw_query(|c| {
                    Ok(
                        c.transaction(|sqlite_c| match drive_to_completion(f(sqlite_c)) {
                            Ok(Continue(v)) => Ok(Continue(v)),
                            Ok(Rollback) => {
                                rolled_back = true;
                                Err(E::from(diesel::result::Error::RollbackTransaction))
                            }
                            Err(e) => Err(e),
                        }),
                    )
                })
                .map_err(E::from)?;

            match inner_result {
                Ok(outcome) => Ok(outcome),
                Err(_) if rolled_back => Ok(Rollback),
                Err(e) => Err(e),
            }
        }
    }

    // Typed accessors. On SQLite the storage stays the generic `openmls_key_value`
    // KV: the label + key encoding (`COMMIT_LOG_SIGNER_PRIVATE_KEY`, bincode of the
    // group id) lives here rather than at the call site, so the on-disk format is
    // byte-identical to what the generic KV path writes. Synchronous bodies under
    // an already-ready future, like the generic accessors above.
    fn set_commit_log_signer_key(
        &self,
        group_id: &[u8],
        signer_key: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        let result = bincode::serialize(group_id)
            .map_err(|_| SqlKeyStoreError::SerializationError)
            .and_then(|key| {
                self.write::<CURRENT_VERSION>(COMMIT_LOG_SIGNER_PRIVATE_KEY, &key, signer_key)
            });
        std::future::ready(result)
    }

    fn commit_log_signer_key<V: Entity<CURRENT_VERSION> + xmtp_common::MaybeSend>(
        &self,
        group_id: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<V>, SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        let result = bincode::serialize(group_id)
            .map_err(|_| SqlKeyStoreError::SerializationError)
            .and_then(|key| self.read::<CURRENT_VERSION, V>(COMMIT_LOG_SIGNER_PRIVATE_KEY, &key));
        std::future::ready(result)
    }

    // Key-package references + wrapper keys: keyed verbatim (public key / hash
    // ref) under their labels, matching the old generic path's on-disk bytes.
    fn set_key_package_reference(
        &self,
        public_key: &[u8],
        hash_ref: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        std::future::ready(self.write::<CURRENT_VERSION>(
            KEY_PACKAGE_REFERENCES,
            public_key,
            hash_ref,
        ))
    }

    fn key_package_reference<V: Entity<CURRENT_VERSION> + xmtp_common::MaybeSend>(
        &self,
        public_key: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<V>, SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        std::future::ready(self.read::<CURRENT_VERSION, V>(KEY_PACKAGE_REFERENCES, public_key))
    }

    fn delete_key_package_reference(
        &self,
        public_key: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        std::future::ready(self.delete::<CURRENT_VERSION>(KEY_PACKAGE_REFERENCES, public_key))
    }

    fn set_key_package_wrapper_key(
        &self,
        hash_ref: &[u8],
        private_key: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        std::future::ready(self.write::<CURRENT_VERSION>(
            KEY_PACKAGE_WRAPPER_PRIVATE_KEY,
            hash_ref,
            private_key,
        ))
    }

    fn key_package_wrapper_key<V: Entity<CURRENT_VERSION> + xmtp_common::MaybeSend>(
        &self,
        hash_ref: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<V>, SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        std::future::ready(
            self.read::<CURRENT_VERSION, V>(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, hash_ref),
        )
    }

    fn delete_key_package_wrapper_key(
        &self,
        hash_ref: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + xmtp_common::MaybeSend
    {
        std::future::ready(
            self.delete::<CURRENT_VERSION>(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, hash_ref),
        )
    }

    #[cfg(feature = "test-utils")]
    fn hash_all(&self) -> Result<Vec<u8>, SqlKeyStoreError> {
        self.conn
            .raw_query(OpenMlsKeyValue::hash_all)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {

    #![allow(unused)]

    use crate::{
        TestDb, TransactionOutcome, XmtpTestDb,
        group_intent::{IntentKind, IntentState, NewGroupIntent},
        prelude::QueryGroupIntent,
    };
    use xmtp_proto::types::GroupId;

    use super::*;

    // Test to ensure that we can use the transaction() callback without requiring a 'static
    // lifetimes
    // This ensures we do not propagate 'static throughout all of our code.
    // have not figured out a good, ergonomic way to pass SqlKeyStore directly into the
    // transaction callback
    struct Foo<C> {
        key_store: SqlKeyStore<C>,
    }

    impl<C> Foo<C>
    where
        C: ConnectionExt,
    {
        async fn long_async_call(&self) {
            xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        async fn db_op(&self) {
            self.long_async_call().await;

            self.key_store
                .transaction(async |conn| {
                    let storage = conn.key_store();
                    storage
                        .db()
                        .insert_group_intent(NewGroupIntent {
                            kind: IntentKind::SendMessage,
                            group_id: GroupId::default(),
                            data: vec![],
                            should_push: false,
                            state: IntentState::ToPublish,
                        })
                        .await
                        .map(Continue)
                })
                .await
                .unwrap();
            self.long_async_call().await;
        }
    }
}
