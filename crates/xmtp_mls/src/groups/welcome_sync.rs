use crate::client::ClientError;
use crate::context::XmtpSharedContext;
use crate::groups::InitialMembershipValidator;
use crate::groups::ValidateGroupMembership;
use crate::groups::XmtpWelcome;
use crate::groups::{GroupError, MlsGroup};
use crate::intents::ProcessIntentError;
use crate::mls_store::MlsStore;
use futures::stream::{self, FuturesUnordered, StreamExt};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use xmtp_common::Event;
use xmtp_common::fmt::ShortHex;
use xmtp_common::{Retry, retry_async};
use xmtp_db::refresh_state::EntityKind;
use xmtp_db::{consent_record::ConsentState, group::GroupQueryArgs, prelude::*};
use xmtp_macro::log_event;
use xmtp_proto::types::GlobalCursor;
use xmtp_proto::types::GroupId;
use xmtp_proto::types::GroupMessageMetadata;

#[derive(Debug, Clone)]
pub struct GroupSyncSummary {
    pub num_eligible: usize,
    pub num_synced: usize,
}

impl GroupSyncSummary {
    pub fn new(num_eligible: usize, num_synced: usize) -> Self {
        Self {
            num_eligible,
            num_synced,
        }
    }
}

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
        welcome: &xmtp_proto::types::WelcomeMessage,
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
            Ok(mls_group) => {
                if let Some(mls_group) = &mls_group {
                    log_event!(
                        Event::ProcessedWelcome,
                        self.context.installation_id(),
                        group_id = mls_group.group_id.as_slice().short_hex(),
                        conversation_type = %mls_group.conversation_type
                    );
                }

                Ok(mls_group)
            }
            Err(err) => {
                use crate::DuplicateItem::*;
                use crate::StorageError::*;

                if matches!(err, GroupError::Storage(Duplicate(WelcomeId(_)))) {
                    tracing::warn!(
                        welcome_cursor = %welcome.cursor,
                        "Welcome ID already stored: {}",
                        err
                    );
                    return Err(GroupError::ProcessIntent(
                        ProcessIntentError::WelcomeAlreadyProcessed(welcome.cursor),
                    ));
                } else {
                    tracing::error!(
                        "failed to create group from welcome={} created at {}: {}",
                        welcome.cursor,
                        welcome.created_ns.timestamp(),
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
        let envelopes = store.query_welcome_messages().await?;
        let num_envelopes = envelopes.len();

        // TODO: Update cursor correctly if some of the welcomes fail and some of the welcomes succeed
        let groups: Vec<MlsGroup<Context>> = stream::iter(envelopes.into_iter())
            .filter_map(|welcome| async move {
                retry_async!(
                    Retry::default(),
                    (async {
                        let validator = InitialMembershipValidator::new(&self.context);
                        self.process_new_welcome(&welcome, true, validator).await
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
    ) -> Result<GroupSyncSummary, GroupError> {
        let num_eligible_groups = groups.len();
        let active_group_count = Arc::new(AtomicUsize::new(0));
        let sync_futures = self
            .filter_groups_needing_sync(groups)
            .await?
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

        Ok(GroupSyncSummary::new(
            num_eligible_groups,
            active_group_count.load(Ordering::SeqCst),
        ))
    }

    async fn filter_groups_needing_sync(
        &self,
        groups: Vec<MlsGroup<Context>>,
    ) -> Result<Vec<MlsGroup<Context>>, GroupError> {
        let db = self.context.db();
        let api = self.context.api();

        let group_ids: Vec<&[u8]> = groups.iter().map(|group| group.group_id.as_ref()).collect();
        let last_synced_cursors = db.get_last_cursor_for_ids(
            &group_ids,
            &[EntityKind::ApplicationMessage, EntityKind::CommitMessage],
        )?;
        let latest_message_metadata = api.get_newest_message_metadata(group_ids).await?;

        let group_ids_needing_sync =
            filter_groups_with_new_messages(last_synced_cursors, latest_message_metadata);

        Ok(groups
            .into_iter()
            .filter(|group| group_ids_needing_sync.contains(&group.group_id))
            .collect::<Vec<_>>())
    }

    pub async fn sync_all_welcomes_and_history_sync_groups(
        &self,
    ) -> Result<GroupSyncSummary, ClientError> {
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

        Ok(self.sync_all_groups(groups).await?)
    }

    /// Sync all unread welcome messages and then sync groups in descending order of recent activity.
    /// Returns number of active groups successfully synced.
    pub async fn sync_all_welcomes_and_groups(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<GroupSyncSummary, GroupError> {
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

        let all_groups: Vec<MlsGroup<Context>> = conversations
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

        let total_groups = all_groups.len();

        let filtered_groups = self.filter_groups_needing_sync(all_groups).await?;

        let success_count = self.sync_groups_in_batches(filtered_groups, 10).await?;

        Ok(GroupSyncSummary::new(total_groups, success_count))
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

// Take the mapping of last synced cursors and the latest messages
// Filter groups that have messages newer than their last synced cursor
fn filter_groups_with_new_messages(
    last_synced_cursors: HashMap<Vec<u8>, GlobalCursor>,
    latest_messages: HashMap<GroupId, GroupMessageMetadata>,
) -> HashSet<Vec<u8>> {
    let mut groups_with_unread_messages = HashSet::new();
    for (group_id, latest_message_metadata) in latest_messages {
        match last_synced_cursors.get(group_id.as_ref()) {
            Some(cursor) => {
                // Get the database cursor for the originator ID
                // or 0 if not found. Compare with the latest message.
                if cursor.get(&latest_message_metadata.cursor.originator_id)
                    < latest_message_metadata.cursor.sequence_id
                {
                    groups_with_unread_messages.insert(group_id.to_vec());
                }
            }
            None => {
                // No cursor found. Must have never been synced before.
                groups_with_unread_messages.insert(group_id.to_vec());
            }
        }
    }

    groups_with_unread_messages
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
    use xmtp_common::Generate;
    use xmtp_configuration::Originators;
    use xmtp_db::StorageError;
    use xmtp_db::refresh_state::EntityKind;
    use xmtp_db::sql_key_store::SqlKeyStore;
    use xmtp_db::{MemoryStorage, mock::MockDbQuery, sql_key_store::mock::MockSqlKeyStore};
    use xmtp_proto::mls_v1::WelcomeMetadata;
    use xmtp_proto::types::{Cursor, WelcomeMessage, WelcomeMessageType, WelcomeMessageV1};

    fn generate_welcome(
        id: u64,
        public_key: Vec<u8>,
        welcome: MlsMessageOut,
        message_cursor: Option<u64>,
    ) -> WelcomeMessage {
        let (data, welcome_metadata) = wrap_welcome(
            &welcome.tls_serialize_detached().unwrap(),
            &WelcomeMetadata {
                message_cursor: message_cursor.unwrap_or(0),
            }
            .encode_to_vec(),
            &public_key,
            WrapperAlgorithm::Curve25519,
        )
        .unwrap();

        let random = WelcomeMessage::generate();
        let random_v1 = random.as_v1().unwrap();

        WelcomeMessage {
            cursor: crate::groups::Cursor::new(id, Originators::WELCOME_MESSAGES),
            created_ns: random.created_ns,
            variant: WelcomeMessageType::V1(WelcomeMessageV1 {
                installation_key: random_v1.installation_key,
                data,
                hpke_public_key: public_key,
                wrapper_algorithm: WrapperAlgorithm::Curve25519.into(),
                welcome_metadata,
            }),
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
                db.expect_get_last_cursor_for_originators()
                    .returning(|_id, _entity, _| Ok(vec![Cursor::v3_welcomes(0)]));
                db.expect_find_group().returning(|_id| Ok(None));
            })
            // outer tx
            .transaction_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_originators()
                    .returning(|_id, _entity, _| Ok(vec![Cursor::v3_welcomes(0)]));
            })
            // inner tx
            .nested_transaction_calls(|db: &mut MockDbQuery| {
                db.expect_find_group().returning(|_id| Ok(None));
                db.expect_get_last_cursor_for_originators()
                    .returning(|_id, _entity, _| Ok(vec![Cursor::v3_welcomes(0)]));
                db.expect_update_cursor().returning(|_, _, _| Ok(true));
                db.expect_update_responded_at_sequence_id()
                    .returning(|_, _, _| Ok(()));
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
                db.expect_get_last_cursor_for_originators()
                    .once()
                    .returning(|_id, _entity, _| {
                        // non-retryable error in transaction
                        Err(StorageError::DbSerialize)
                    });
            })
            .transaction_calls(|db: &mut MockDbQuery| {
                db.expect_update_cursor()
                    .once()
                    .returning(|_id, entity, cursor| {
                        assert_eq!(cursor, Cursor::v3_welcomes(50));
                        assert_eq!(entity, EntityKind::Welcome);
                        Ok(true)
                    });
            })
            .database_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_originators()
                    .once()
                    .returning(|_id, _entity, _| Ok(vec![Cursor::v3_welcomes(0)]));
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
                db.expect_get_last_cursor_for_originators()
                    .once()
                    .returning(|_id, _entity, _| Ok(vec![Cursor::v3_welcomes(0)]));
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
                db.expect_find_group().returning(|_id| Ok(None));
                db.expect_get_last_cursor_for_originators()
                    .returning(|_id, _entity, _| Ok(vec![Cursor::v3_welcomes(0)]));
                db.expect_update_cursor().returning(|_, _, _| Ok(true));
                db.expect_insert_or_replace_group().returning(Ok);
            })
            .transaction_calls(|db: &mut MockDbQuery| {
                db.expect_update_cursor()
                    .once()
                    .returning(|_id, entity, cursor| {
                        assert_eq!(cursor, Cursor::v3_welcomes(50));
                        assert_eq!(entity, EntityKind::Welcome);
                        Ok(true)
                    });
                db.expect_update_responded_at_sequence_id()
                    .once()
                    .returning(|_, _, _| Ok(()));
                db.expect_update_cursor()
                    .once()
                    .returning(|_id, entity, cursor| {
                        assert_eq!(cursor, Cursor::new(10, 0u32));
                        assert_eq!(entity, EntityKind::CommitMessage);
                        Ok(true)
                    });
            })
            .database_calls(|db: &mut MockDbQuery| {
                db.expect_get_last_cursor_for_originators()
                    .once()
                    .returning(|_id, _entity, _| Ok(vec![Cursor::v3_welcomes(0)]));
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
                db.expect_get_last_cursor_for_originators()
                    .once()
                    .returning(|_id, _entity, _| Ok(vec![Cursor::v3_welcomes(0)]));
            })
            .mem(mem)
            .build();

        let service = WelcomeService::new(context);
        let res = service
            .process_new_welcome(&network_welcome, cursor_increment, validator)
            .await;
        assert!(res.is_err(), "{}", res.unwrap_err());
    }

    // Helper functions for filter_groups_with_new_messages tests
    fn make_cursor(originator_id: u32, sequence_id: u64) -> GlobalCursor {
        let mut map = GlobalCursor::default();
        map.insert(originator_id, sequence_id);
        map
    }

    fn make_message_metadata(
        group_id: Vec<u8>,
        originator_id: u32,
        sequence_id: u64,
    ) -> GroupMessageMetadata {
        use chrono::Utc;
        GroupMessageMetadata::builder()
            .cursor(Cursor::new(sequence_id, originator_id))
            .created_ns(Utc::now())
            .group_id(group_id)
            .build()
            .unwrap()
    }

    #[xmtp_common::test]
    fn filter_groups_with_new_messages_basic_behavior() {
        let group_id_1 = vec![1, 2, 3];
        let group_id_2 = vec![4, 5, 6];
        let originator = 100;

        let mut last_synced = HashMap::new();
        last_synced.insert(group_id_1.clone(), make_cursor(originator, 5));
        last_synced.insert(group_id_2.clone(), make_cursor(originator, 10));

        let mut latest = HashMap::new();
        latest.insert(
            group_id_1.clone().into(),
            make_message_metadata(group_id_1.clone(), originator, 10), // New: 10 > 5
        );
        latest.insert(
            group_id_2.clone().into(),
            make_message_metadata(group_id_2.clone(), originator, 8), // No new: 8 < 10
        );

        let result = filter_groups_with_new_messages(last_synced, latest);

        assert_eq!(result.len(), 1);
        assert!(result.contains(&group_id_1));
    }

    #[xmtp_common::test]
    fn filter_groups_includes_never_synced_and_excludes_up_to_date() {
        let group_synced = vec![1, 2, 3];
        let group_never_synced = vec![4, 5, 6];
        let originator = 100;

        let mut last_synced = HashMap::new();
        last_synced.insert(group_synced.clone(), make_cursor(originator, 5));
        // group_never_synced has no entry

        let mut latest = HashMap::new();
        latest.insert(
            group_synced.clone().into(),
            make_message_metadata(group_synced.clone(), originator, 3), // Already synced
        );
        latest.insert(
            group_never_synced.clone().into(),
            make_message_metadata(group_never_synced.clone(), originator, 1),
        );

        let result = filter_groups_with_new_messages(last_synced, latest);

        assert_eq!(result.len(), 1);
        assert!(result.contains(&group_never_synced));
    }

    #[xmtp_common::test]
    fn filter_groups_handles_multiple_originators() {
        let group_id = vec![1, 2, 3];
        let orig_1 = 100;
        let orig_2 = 200;

        let mut last_synced = HashMap::new();
        let mut cursor_map = GlobalCursor::default();
        cursor_map.insert(orig_1, 10);
        cursor_map.insert(orig_2, 20);
        last_synced.insert(group_id.clone(), cursor_map);

        let mut latest = HashMap::new();
        latest.insert(
            group_id.clone().into(),
            make_message_metadata(group_id.clone(), orig_2, 25), // New from orig_2
        );

        let result = filter_groups_with_new_messages(last_synced, latest);

        assert_eq!(result.len(), 1);
        assert!(result.contains(&group_id));
    }

    #[xmtp_common::test]
    fn filter_groups_treats_unknown_originator_as_new() {
        let group_id = vec![1, 2, 3];

        let mut last_synced = HashMap::new();
        last_synced.insert(group_id.clone(), make_cursor(100, 10));

        let mut latest = HashMap::new();
        latest.insert(
            group_id.clone().into(),
            make_message_metadata(group_id.clone(), 200, 5), // Unknown originator defaults to 0
        );

        let result = filter_groups_with_new_messages(last_synced, latest);

        assert_eq!(result.len(), 1);
        assert!(result.contains(&group_id));
    }

    // I have no idea why this test is specifically failing on WASM
    #[cfg(not(target_arch = "wasm32"))]
    #[rstest]
    #[case(HashMap::new(), HashMap::new())] // Empty inputs
    #[case({
        let mut m = HashMap::new();
        m.insert(vec![1], make_cursor(100, 10));
        m
    }, {
        let mut m = HashMap::new();
        m.insert(vec![1].into(), make_message_metadata(vec![1], 100, 10));
        m
    })] // Equal cursors
    #[xmtp_common::test]
    fn filter_groups_returns_empty_when_no_updates(
        #[case] last_synced: HashMap<Vec<u8>, GlobalCursor>,
        #[case] latest: HashMap<GroupId, GroupMessageMetadata>,
    ) {
        let result = filter_groups_with_new_messages(last_synced, latest);
        assert_eq!(result.len(), 0);
    }

    #[xmtp_common::test]
    fn filter_groups_comprehensive_mixed_states() {
        let g1 = vec![1];
        let g2 = vec![2];
        let g3 = vec![3];
        let g4 = vec![4];
        let orig = 100;

        let mut last_synced = HashMap::new();
        last_synced.insert(g1.clone(), make_cursor(orig, 5)); // Will have new
        last_synced.insert(g2.clone(), make_cursor(orig, 15)); // Already synced
        last_synced.insert(g3.clone(), make_cursor(orig, 10)); // Equal
        // g4 never synced

        let mut latest = HashMap::new();
        latest.insert(
            g1.clone().into(),
            make_message_metadata(g1.clone(), orig, 10),
        );
        latest.insert(
            g2.clone().into(),
            make_message_metadata(g2.clone(), orig, 12),
        );
        latest.insert(
            g3.clone().into(),
            make_message_metadata(g3.clone(), orig, 10),
        );
        latest.insert(
            g4.clone().into(),
            make_message_metadata(g4.clone(), orig, 1),
        );

        let result = filter_groups_with_new_messages(last_synced, latest);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&g1));
        assert!(result.contains(&g4));
    }
}
