use crate::client::ClientError;
use crate::context::XmtpSharedContext;
use crate::groups::InitialMembershipValidator;
use crate::groups::ValidateGroupMembership;
use crate::groups::XmtpWelcome;
use crate::groups::{GroupError, MlsGroup};
use crate::mls_store::MlsStore;
use futures::stream::{self, FuturesUnordered, StreamExt};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use xmtp_common::{Retry, retry_async};
use xmtp_db::{consent_record::ConsentState, group::GroupQueryArgs, prelude::*};
use xmtp_proto::xmtp::mls::api::v1::{WelcomeMessage, welcome_message};

#[derive(Clone)]
pub struct WelcomeService<Context> {
    context: Context,
}

impl<Context> WelcomeService<Context> {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

impl<Context> WelcomeService<Context>
where
    Context: XmtpSharedContext,
{
    /// Internal API to process a unread welcome message and convert to a group.
    /// In a database transaction, increments the cursor for a given installation and
    /// applies the update after the welcome processed successfully.
    pub(crate) async fn process_new_welcome(
        &self,
        welcome: &welcome_message::V1,
        cursor_increment: bool,
        validator: impl ValidateGroupMembership,
    ) -> Result<Option<MlsGroup<Context>>, GroupError> {
        let result = XmtpWelcome::builder()
            .context(self.context.clone())
            .welcome(welcome)
            .cursor_increment(cursor_increment)
            .validator(validator)
            .process()
            .await;

        match result {
            Ok(mls_group) => Ok(mls_group),
            Err(err) => {
                use crate::DuplicateItem::*;
                use crate::StorageError::*;

                if matches!(err, GroupError::Storage(Duplicate(WelcomeId(_)))) {
                    tracing::warn!(
                        "failed to create group from welcome due to duplicate welcome ID: {}",
                        err
                    );
                } else {
                    tracing::error!(
                        "failed to create group from welcome created at {}: {}",
                        welcome.created_ns,
                        err
                    );
                }

                Err(err)
            }
        }
    }

    /// Download all unread welcome messages and converts to a group struct, ignoring malformed messages.
    /// Returns any new groups created in the operation
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Context>>, GroupError> {
        let db = self.context.db();
        let store = MlsStore::new(self.context.clone());
        let envelopes = store.query_welcome_messages(&db).await?;
        let num_envelopes = envelopes.len();

        let groups: Vec<MlsGroup<Context>> = stream::iter(envelopes.into_iter())
            .filter_map(|envelope: WelcomeMessage| async {
                let welcome_v1 = match envelope.version {
                    Some(welcome_message::Version::V1(v1)) => v1,
                    _ => {
                        tracing::error!(
                            "failed to extract welcome message, invalid payload only v1 supported."
                        );
                        return None;
                    }
                };
                retry_async!(
                    Retry::default(),
                    (async {
                        let validator = InitialMembershipValidator::new(&self.context);
                        self.process_new_welcome(&welcome_v1, true, validator).await
                    })
                )
                .ok()?
            })
            .collect()
            .await;

        // Rotate the keys regardless of whether the welcomes failed or succeeded. It is better to over-rotate than
        // to under-rotate, as the latter risks leaving expired key packages on the network. We already have a max
        // rotation interval.
        if num_envelopes > 0 {
            self.context.identity().queue_key_rotation(&db).await?;
        }

        Ok(groups)
    }

    /// Sync all groups for the current installation and return the number of groups that were synced.
    /// Only active groups will be synced.
    pub async fn sync_all_groups(
        &self,
        groups: Vec<MlsGroup<Context>>,
    ) -> Result<usize, GroupError> {
        let active_group_count = Arc::new(AtomicUsize::new(0));

        let sync_futures = groups
            .into_iter()
            .map(|group| {
                let active_group_count = Arc::clone(&active_group_count);
                async move {
                    tracing::info!(
                        inbox_id = self.context.inbox_id(),
                        "[{}] syncing group",
                        self.context.inbox_id()
                    );
                    let is_active = group
                        .load_mls_group_with_lock_async(|mls_group| async move {
                            Ok::<bool, GroupError>(mls_group.is_active())
                        })
                        .await?;
                    if is_active {
                        group.sync_with_conn().await?;
                        group.maybe_update_installations(None).await?;
                        active_group_count.fetch_add(1, Ordering::SeqCst);
                    }

                    Ok::<(), GroupError>(())
                }
            })
            .collect::<FuturesUnordered<_>>();

        sync_futures
            .collect::<Vec<Result<_, _>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        Ok(active_group_count.load(Ordering::SeqCst))
    }

    pub async fn sync_all_welcomes_and_history_sync_groups(&self) -> Result<usize, ClientError> {
        let db = self.context.db();
        self.sync_welcomes().await?;
        let groups = db
            .all_sync_groups()?
            .into_iter()
            .map(|g| {
                MlsGroup::new(
                    self.context.clone(),
                    g.id,
                    g.dm_id,
                    g.conversation_type,
                    g.created_at_ns,
                )
            })
            .collect();
        let active_groups_count = self.sync_all_groups(groups).await?;

        Ok(active_groups_count)
    }

    /// Sync all unread welcome messages and then sync groups in descending order of recent activity.
    /// Returns number of active groups successfully synced.
    pub async fn sync_all_welcomes_and_groups(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<usize, GroupError> {
        let db = self.context.db();

        if let Err(err) = self.sync_welcomes().await {
            tracing::warn!(?err, "sync_welcomes failed, continuing with group sync");
        }
        let query_args = GroupQueryArgs {
            consent_states,
            include_duplicate_dms: true,
            include_sync_groups: true,
            ..GroupQueryArgs::default()
        };

        let conversations = db.fetch_conversation_list(query_args)?;

        let groups: Vec<MlsGroup<Context>> = conversations
            .into_iter()
            .map(|c| {
                MlsGroup::new(
                    self.context.clone(),
                    c.id,
                    c.dm_id,
                    c.conversation_type,
                    c.created_at_ns,
                )
            })
            .collect();

        let success_count = self.sync_groups_in_batches(groups, 10).await?;

        Ok(success_count)
    }

    /// Sync groups concurrently with a limit. Returns success count.
    pub async fn sync_groups_in_batches(
        &self,
        groups: Vec<MlsGroup<Context>>,
        max_concurrency: usize,
    ) -> Result<usize, GroupError> {
        let active_group_count = Arc::new(AtomicUsize::new(0));
        let failed_group_count = Arc::new(AtomicUsize::new(0));

        stream::iter(groups)
            .for_each_concurrent(max_concurrency, |group| {
                let group = group.clone();
                let active_group_count = Arc::clone(&active_group_count);
                let failed_group_count = Arc::clone(&failed_group_count);
                let inbox_id = self.context.inbox_id();

                async move {
                    tracing::info!(inbox_id, "[{}] syncing group", inbox_id);

                    let is_active_res = group
                        .load_mls_group_with_lock_async(|mls_group| async move {
                            Ok::<bool, GroupError>(mls_group.is_active())
                        })
                        .await;

                    match is_active_res {
                        Ok(is_active) if is_active => {
                            if let Err(err) = group.sync_with_conn().await {
                                tracing::warn!(?err, "sync_with_conn failed");
                                failed_group_count.fetch_add(1, Ordering::SeqCst);
                                return;
                            }

                            if let Err(err) = group.maybe_update_installations(None).await {
                                tracing::warn!(?err, "maybe_update_installations failed");
                                failed_group_count.fetch_add(1, Ordering::SeqCst);
                                return;
                            }

                            active_group_count.fetch_add(1, Ordering::SeqCst);
                        }
                        Ok(_) => { /* group inactive, skip */ }
                        Err(err) => {
                            tracing::warn!(?err, "load_mls_group_with_lock_async failed");
                            failed_group_count.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }
            })
            .await;

        Ok(active_group_count.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::mls_ext::WrapperAlgorithm;
    use crate::groups::mls_ext::wrap_welcome;
    use crate::groups::test::NoopValidator;
    use crate::test::mock::*;
    use derive_builder::Builder;
    use openmls::prelude::MlsMessageOut;
    use prost::Message;
    use rstest::*;
    use tls_codec::Serialize;
    use xmtp_db::StorageError;
    use xmtp_db::refresh_state::EntityKind;
    use xmtp_db::sql_key_store::SqlKeyStore;
    use xmtp_db::{MemoryStorage, mock::MockDbQuery, sql_key_store::mock::MockSqlKeyStore};
    use xmtp_proto::mls_v1::WelcomeMetadata;

    fn generate_welcome(
        id: u64,
        public_key: Vec<u8>,
        welcome: MlsMessageOut,
        message_cursor: Option<u64>,
    ) -> welcome_message::V1 {
        let w = wrap_welcome(
            &welcome.tls_serialize_detached().unwrap(),
            &public_key,
            &WrapperAlgorithm::Curve25519,
        )
        .unwrap();

        let wrapped_welcome_metadata: Vec<u8> = if let Some(cursor) = message_cursor {
            let welcome_metadata = WelcomeMetadata {
                message_cursor: cursor,
            }
            .encode_to_vec();
            wrap_welcome(
                &welcome_metadata,
                &public_key,
                &WrapperAlgorithm::Curve25519,
            )
            .unwrap()
        } else {
            Vec::new()
        };

        welcome_message::V1 {
            id,
            created_ns: 0,
            installation_key: vec![0],
            data: w,
            hpke_public_key: public_key,
            wrapper_algorithm: WrapperAlgorithm::Curve25519.into(),
            welcome_metadata: wrapped_welcome_metadata,
        }
    }

    #[derive(Builder)]
    #[builder(
        pattern = "owned",
        setter(strip_option),
        build_fn(name = "inner_build")
    )]
    struct TestWelcomeSetup<DbF, TxF, TxF2, V> {
        database_calls: DbF,
        transaction_calls: TxF,
        nested_transaction_calls: TxF2,
        mem: Arc<SqlKeyStore<MemoryStorage>>,
        context: NewMockContext,
        validator: V,
    }

    impl<DbF, TxF, TxF2, V> TestWelcomeSetup<DbF, TxF, TxF2, V> {
        fn builder() -> TestWelcomeSetupBuilder<DbF, TxF, TxF2, V> {
            TestWelcomeSetupBuilder::default()
        }
    }

    impl<DbF, TxF, TxF2, V> TestWelcomeSetupBuilder<DbF, TxF, TxF2, V>
    where
        DbF: FnMut(&mut MockDbQuery) + Send + 'static,
        TxF: FnMut(&mut MockDbQuery) + Send + 'static,
        TxF2: FnMut(&mut MockDbQuery) + Send + 'static,
        V: ValidateGroupMembership,
    {
        fn build(self) -> (impl XmtpSharedContext, impl ValidateGroupMembership) {
            let this = self.inner_build().unwrap();
            let mut tx_functions = this.transaction_calls;
            let mut nested_tx_functions = this.nested_transaction_calls;
            let mem = &this.mem;
            // db calls inside of transaction
            let db = {
                let mut db = MockDbQuery::new();
                tx_functions(&mut db);
                nested_tx_functions(&mut db);
                Arc::new(db)
            };
            let mut mls_store = MockTransactionalKeyStore::default();
            let db = db.clone();
            mls_store.expect_key_store().returning({
                let db = db.clone();
                let mem = mem.clone();
                move || {
                    let mut mls_store = MockTransactionalKeyStore::default();
                    mls_store.expect_key_store().returning({
                        let db = db.clone();
                        let mem = mem.clone();
                        move || {
                            let mls_store = MockTransactionalKeyStore::default();
                            MockSqlKeyStore::new(db.clone(), mls_store, mem.clone())
                        }
                    });
                    MockSqlKeyStore::new(db.clone(), mls_store, mem.clone())
                }
            });

            let key_store = MockSqlKeyStore::new(db.clone(), mls_store, mem.clone());
            let mut context = this.context.replace_mls_store(key_store);

            // db calls outside tx
            let mut database_calls = this.database_calls;
            context.store.expect_db().returning({
                move || {
                    let mut mock_db = MockDbQuery::new();
                    database_calls(&mut mock_db);
                    mock_db
                }
            });
            // after this point everything is immutable
            (Arc::new(context), this.validator)
        }
    }

    #[rstest]
    #[xmtp_common::test]
    async fn happy_path(context: NewMockContext) {
        let mem = Arc::new(SqlKeyStore::new(MemoryStorage::default()));
        let client = create_mls_client(mem.as_ref());
        let (kp, mls_welcome) = client.join_group();
        let network_welcome = generate_welcome(
            50,
            kp.hpke_init_key().as_slice().to_vec(),
            mls_welcome,
            None,
        );

        let (context, validator) = TestWelcomeSetup::builder()
            .validator(NoopValidator)
            .context(context)
            .database_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_id()
                    .returning(|_id, _entity| Ok(0));
                db.expect_find_group().returning(|_id| Ok(None));
            })
            // outer tx
            .transaction_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_id()
                    .returning(|_id, _entity| Ok(0));
            })
            // inner tx
            .nested_transaction_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_id()
                    .returning(|_id, _entity| Ok(0));
                db.expect_update_cursor().returning(|_, _, _| Ok(true));
                db.expect_insert_or_replace_group().returning(Ok);
            })
            .mem(mem)
            .build();

        let service = WelcomeService::new(context);
        let res = service
            .process_new_welcome(&network_welcome, true, validator)
            .await;
        assert!(res.is_ok(), "{}", res.unwrap_err());
    }

    #[rstest]
    #[xmtp_common::test]
    async fn increments_cursor_on_non_retryable_in_tx(context: NewMockContext) {
        let mem = Arc::new(SqlKeyStore::new(MemoryStorage::default()));
        let client = create_mls_client(mem.as_ref());
        let (kp, mls_welcome) = client.join_group();
        let network_welcome = generate_welcome(
            50,
            kp.hpke_init_key().as_slice().to_vec(),
            mls_welcome,
            None,
        );

        let (context, validator) = TestWelcomeSetup::builder()
            .validator(NoopValidator)
            .context(context)
            .nested_transaction_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_id()
                    .once()
                    .returning(|_id, _entity| {
                        // non-retryable error in transaction
                        Err(StorageError::DbSerialize)
                    });
            })
            .transaction_calls(|db: &mut MockDbQuery| {
                db.expect_update_cursor()
                    .once()
                    .returning(|_id, entity, cursor| {
                        assert_eq!(cursor, 50);
                        assert_eq!(entity, EntityKind::Welcome);
                        Ok(true)
                    });
            })
            .database_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_id()
                    .once()
                    .returning(|_id, _entity| Ok(0));
                db.expect_find_group().once().returning(|_id| Ok(None));
            })
            .mem(mem)
            .build();

        let service = WelcomeService::new(context);
        let res = service
            .process_new_welcome(&network_welcome, true, validator)
            .await;
        assert!(res.is_err(), "{}", res.unwrap_err());
    }

    /// Validator which always returns a non-retryable error
    struct NonRetryableValidator;
    impl ValidateGroupMembership for NonRetryableValidator {
        async fn check_initial_membership(
            &self,
            _welcome: &openmls::prelude::StagedWelcome,
        ) -> Result<(), GroupError> {
            Err(GroupError::NoPSKSupport)
        }
    }

    struct RetryableValidator;
    impl ValidateGroupMembership for RetryableValidator {
        async fn check_initial_membership(
            &self,
            _welcome: &openmls::prelude::StagedWelcome,
        ) -> Result<(), GroupError> {
            Err(GroupError::LockUnavailable)
        }
    }

    #[rstest]
    #[xmtp_common::test]
    async fn increments_cursor_on_non_retryable_during_validation(context: NewMockContext) {
        let mem = Arc::new(SqlKeyStore::new(MemoryStorage::default()));
        let client = create_mls_client(mem.as_ref());
        let (kp, mls_welcome) = client.join_group();
        let network_welcome = generate_welcome(
            50,
            kp.hpke_init_key().as_slice().to_vec(),
            mls_welcome,
            None,
        );

        let (context, validator) = TestWelcomeSetup::builder()
            .validator(NonRetryableValidator)
            .context(context)
            .nested_transaction_calls(|_db: &mut MockDbQuery| {})
            .transaction_calls(|_db: &mut MockDbQuery| ())
            .database_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_id()
                    .once()
                    .returning(|_id, _entity| Ok(0));
                db.expect_update_cursor()
                    .once()
                    .returning(|_, _, _| Ok(true));
            })
            .mem(mem)
            .build();

        let service = WelcomeService::new(context);
        let res = service
            .process_new_welcome(&network_welcome, true, validator)
            .await;
        assert!(res.is_err(), "{}", res.unwrap_err());
    }

    #[rstest]
    #[xmtp_common::test]
    async fn increments_message_cursor_from_welcome_metadata(context: NewMockContext) {
        let mem = Arc::new(SqlKeyStore::new(MemoryStorage::default()));
        let client = create_mls_client(mem.as_ref());
        let (kp, mls_welcome) = client.join_group();
        let network_welcome = generate_welcome(
            50,
            kp.hpke_init_key().as_slice().to_vec(),
            mls_welcome,
            Some(10),
        );

        let (context, validator) = TestWelcomeSetup::builder()
            .validator(NoopValidator)
            .context(context)
            .nested_transaction_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_id()
                    .returning(|_id, _entity| Ok(0));
                db.expect_update_cursor().returning(|_, _, _| Ok(true));
                db.expect_insert_or_replace_group().returning(Ok);
            })
            .transaction_calls(|db: &mut MockDbQuery| {
                db.expect_update_cursor()
                    .once()
                    .returning(|_id, entity, cursor| {
                        assert_eq!(cursor, 50);
                        assert_eq!(entity, EntityKind::Welcome);
                        Ok(true)
                    });
                db.expect_update_cursor()
                    .once()
                    .returning(|_id, entity, cursor| {
                        assert_eq!(cursor, 10);
                        assert_eq!(entity, EntityKind::Group);
                        Ok(true)
                    });
            })
            .database_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_id()
                    .once()
                    .returning(|_id, _entity| Ok(0));
                db.expect_find_group().once().returning(|_id| Ok(None));
            })
            .mem(mem)
            .build();

        let service = WelcomeService::new(context);
        let res = service
            .process_new_welcome(&network_welcome, true, validator)
            .await;
        assert!(res.is_ok(), "{}", res.unwrap_err());
    }

    #[rstest]
    #[case::non_retryable_disallow_cursor(NonRetryableValidator, false)]
    #[case::retryable_cursor_increment_allowed(RetryableValidator, true)]
    #[xmtp_common::test]
    async fn does_not_increment(
        context: NewMockContext,
        #[case] validator: impl ValidateGroupMembership,
        #[case] cursor_increment: bool,
    ) {
        let mem = Arc::new(SqlKeyStore::new(MemoryStorage::default()));
        let client = create_mls_client(mem.as_ref());
        let (kp, mls_welcome) = client.join_group();
        let network_welcome = generate_welcome(
            50,
            kp.hpke_init_key().as_slice().to_vec(),
            mls_welcome,
            None,
        );

        let (context, validator) = TestWelcomeSetup::builder()
            .validator(validator)
            .context(context)
            .nested_transaction_calls(|_db: &mut MockDbQuery| {})
            .transaction_calls(|_db: &mut MockDbQuery| {})
            .database_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_id()
                    .once()
                    .returning(|_id, _entity| Ok(0));
            })
            .mem(mem)
            .build();

        let service = WelcomeService::new(context);
        let res = service
            .process_new_welcome(&network_welcome, cursor_increment, validator)
            .await;
        assert!(res.is_err(), "{}", res.unwrap_err());
    }
}
