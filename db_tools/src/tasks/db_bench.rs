use anyhow::{Result, bail};
use std::{cmp::Ordering, collections::HashMap};
use xmtp_db::{
    ConnectionError, ConnectionExt, XmtpDb,
    association_state::QueryAssociationStateCache,
    consent_record::{ConsentState, ConsentType, QueryConsentRecord, StoredConsentRecord},
    conversation_list::QueryConversationList,
    diesel::Connection,
    group::{ConversationType, GroupQueryArgs, QueryGroup, StoredGroup},
    group_intent::{IntentKind, IntentState, QueryGroupIntent},
    group_message::{MsgQueryArgs, QueryGroupMessage, RelationQuery},
    identity_update::QueryIdentityUpdates,
    key_package_history::QueryKeyPackageHistory,
    local_commit_log::{LocalCommitLogOrder, QueryLocalCommitLog},
    proto::types::{Cursor, GlobalCursor},
    refresh_state::{EntityKind, QueryRefreshState},
    remote_commit_log::{QueryRemoteCommitLog, RemoteCommitLogOrder},
};

macro_rules! bench {
    ($self:ident, $fn:ident($($args:expr),*)) => {{
        let key = stringify!($fn($($args),*));
        let store = $self.store.clone();
        let result = $self.bench_with_key(key, || store.db().$fn($($args),*));
        result
    }};
}

pub struct DbBencher<Db> {
    rand_dm: Option<StoredGroup>,
    rand_group: Option<StoredGroup>,
    rand_inbox_id: Option<String>,
    measurements: HashMap<String, f32>,
    store: Db,
}

impl<Db> DbBencher<Db>
where
    Db: XmtpDb + Clone,
    <Db as XmtpDb>::Connection: ConnectionExt,
{
    pub fn new(store: Db) -> Result<Self> {
        let mut dms = store.db().find_groups_by_id_paged(
            GroupQueryArgs {
                conversation_type: Some(ConversationType::Dm),
                ..Default::default()
            },
            0,
        )?;
        let mut groups = store.db().find_groups_by_id_paged(
            GroupQueryArgs {
                conversation_type: Some(ConversationType::Group),
                ..Default::default()
            },
            0,
        )?;

        // Try to get a random inbox_id from identity updates or association state
        let rand_inbox_id = groups.first().map(|g| g.added_by_inbox_id.clone());

        Ok(Self {
            rand_dm: dms.pop(),
            rand_group: groups.pop(),
            rand_inbox_id,
            store,
            measurements: HashMap::default(),
        })
    }

    fn bench_with_key<T, F>(&mut self, key: &str, mut f: F) -> T
    where
        F: FnMut() -> T,
    {
        const ITERATIONS: u32 = 10;

        let mut total_elapsed = 0u128;
        let mut last_result = None;

        for _ in 0..ITERATIONS {
            let start = std::time::Instant::now();
            let result = f();
            let elapsed = start.elapsed();
            total_elapsed += elapsed.as_nanos();
            last_result = Some(result);
        }

        let average = (total_elapsed / ITERATIONS as u128) as u64;
        // Convert to milliseconds.
        self.measurements
            .insert(key.to_string(), average as f32 / 1_000_000.);

        last_result.unwrap()
    }

    pub fn bench(&mut self) -> Result<Vec<Result<()>>> {
        let mut results = vec![];
        let result = self.store.conn().raw_query_write(|conn| {
            conn.transaction(|_txn| {
                results.push(self.bench_group_queries());
                results.push(self.bench_group_intent_queries());
                results.push(self.bench_consent_queries());
                results.push(self.bench_message_queries());
                results.push(self.bench_association_state_queries());
                results.push(self.bench_identity_update_queries());
                results.push(self.bench_refresh_state_queries());
                results.push(self.bench_key_package_history_queries());
                results.push(self.bench_conversation_list_queries());
                results.push(self.bench_commit_log_queries());

                Err::<(), xmtp_db::diesel::result::Error>(
                    xmtp_db::diesel::result::Error::RollbackTransaction,
                )
            })
        });

        match result {
            Err(ConnectionError::Database(xmtp_db::diesel::result::Error::RollbackTransaction)) => {
                // Expected
            }
            result => result?,
        }

        self.print_results();

        for result in &results {
            let _ = result.as_ref().inspect_err(|e| {
                tracing::warn!("{e:?}");
            });
        }

        Ok(results)
    }

    fn print_results(&self) {
        // Sort measurements by execution time (greatest to least)
        let mut sorted_measurements: Vec<_> = self.measurements.iter().collect();
        // Send divide-by-zeroes to the bottom
        sorted_measurements.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(Ordering::Less));

        println!("\n{}", "=".repeat(80));
        println!("{:^80}", "Database Benchmark Results");
        println!("{}", "=".repeat(80));
        println!("{:<50} {:>12} {:>10}", "Query", "Time (ms)", "Relative");
        println!("{}", "-".repeat(80));

        let max_time = sorted_measurements.first().map(|(_, t)| **t).unwrap_or(1.0);

        for (query, time_ms) in sorted_measurements.into_iter() {
            let relative = time_ms / max_time;
            let bar_length = (relative * 20.0) as usize;
            let bar = "█".repeat(bar_length);

            println!(
                "{:<50} {:>12.3} {:>9.1}% {}",
                query,
                time_ms,
                relative * 100.0,
                bar
            );
        }

        println!("{}", "=".repeat(80));
        println!(
            "Total queries benchmarked: {} | Average time: {:.3} ms\n",
            self.measurements.len(),
            self.measurements.values().sum::<f32>() / self.measurements.len() as f32
        );
    }

    pub fn bench_message_queries(&mut self) -> Result<()> {
        let Some(group) = self.rand_group.clone() else {
            bail!("No groups to run message queries on.");
        };

        // Basic message queries
        bench!(
            self,
            get_group_messages(&group.id, &MsgQueryArgs::default())
        )?;
        bench!(
            self,
            count_group_messages(&group.id, &MsgQueryArgs::default())
        )?;
        bench!(self, group_messages_paged(&MsgQueryArgs::default(), 0))?;
        bench!(
            self,
            get_group_messages_with_reactions(&group.id, &MsgQueryArgs::default())
        )?;
        bench!(self, get_sync_group_messages(&group.id, 0))?;

        // Try to get a message to use for further benchmarks
        let messages = self
            .store
            .db()
            .get_group_messages(&group.id, &MsgQueryArgs::default())?;

        if let Some(message) = messages.first() {
            let message_id = message.id.clone();
            let message_ids = vec![message_id.as_slice()];

            // Message retrieval benchmarks
            bench!(self, get_group_message(&message_id))?;
            bench!(self, write_conn_get_group_message(&message_id))?;

            // Relation benchmarks
            bench!(
                self,
                get_inbound_relations(&group.id, &message_ids, RelationQuery::default())
            )?;
            bench!(self, get_outbound_relations(&group.id, &message_ids))?;
            bench!(
                self,
                get_inbound_relation_counts(&group.id, &message_ids, RelationQuery::default())
            )?;

            // Timestamp/cursor based queries
            bench!(
                self,
                get_group_message_by_timestamp(&group.id, message.sent_at_ns)
            )?;

            // Delivery status updates (non-destructive operations should be safe to benchmark)
            bench!(
                self,
                set_delivery_status_to_published(
                    &message_id,
                    0,
                    Cursor {
                        sequence_id: 0,
                        originator_id: 0
                    },
                    None
                )
            )?;
            bench!(self, set_delivery_status_to_failed(&message_id))?;
        }

        // Query latest message times by sender
        bench!(self, get_latest_message_times_by_sender(&group.id, &[]))?;

        // Cleanup operations
        bench!(self, delete_expired_messages())?;

        // Messages newer than (with empty hashmap for baseline)
        let empty_cursors: HashMap<Vec<u8>, GlobalCursor> = HashMap::new();
        bench!(self, messages_newer_than(&empty_cursors))?;

        Ok(())
    }

    pub fn bench_group_queries(&mut self) -> Result<()> {
        let Some(group) = self.rand_group.clone() else {
            bail!("No groups to run group queries on.");
        };
        bench!(self, get_conversation_ids_for_remote_log_download())?;
        bench!(self, get_conversation_ids_for_fork_check())?;
        bench!(self, get_conversation_ids_for_requesting_readds())?;
        bench!(self, get_conversation_ids_for_responding_readds())?;
        bench!(self, get_conversation_type(&group.id))?;
        bench!(self, find_groups(GroupQueryArgs::default()))?;
        bench!(self, find_groups_by_id_paged(GroupQueryArgs::default(), 0))?;
        bench!(self, all_sync_groups())?;
        bench!(self, primary_sync_group())?;
        bench!(self, find_group(&group.id))?;
        bench!(self, get_rotated_at_ns(group.id.clone()))?;
        bench!(self, update_rotated_at_ns(group.id.clone()))?;
        bench!(self, get_installations_time_checked(group.id.clone()))?;
        bench!(
            self,
            update_message_disappearing_from_ns(group.id.clone(), Some(1000))
        )?;
        bench!(
            self,
            update_message_disappearing_in_ns(group.id.clone(), Some(1000))
        )?;

        Ok(())
    }

    fn bench_consent_queries(&mut self) -> Result<()> {
        let Some(group) = &self.rand_group else {
            bail!("No group to lookup DMs on");
        };
        let group_id = hex::encode(&group.id);

        let new_consent = StoredConsentRecord {
            consented_at_ns: 0,
            entity: group_id.clone(),
            entity_type: ConsentType::ConversationId,
            state: ConsentState::Allowed,
        };

        bench!(
            self,
            get_consent_record(group_id.clone(), ConsentType::ConversationId)
        )?;
        bench!(self, consent_records())?;
        bench!(self, consent_records_paged(100, 0))?;
        bench!(self, insert_newer_consent_record(new_consent.clone()))?;
        bench!(
            self,
            insert_or_replace_consent_records(std::slice::from_ref(&new_consent))
        )?;
        bench!(
            self,
            maybe_insert_consent_record_return_existing(&new_consent.clone())
        )?;
        if let Some(dm) = &self.rand_dm {
            let Some(dm_id) = dm.dm_id.clone() else {
                bail!("Unexpected: DM does not have a dm_id");
            };
            bench!(self, find_consent_by_dm_id(&dm_id))?;
        }

        Ok(())
    }

    fn bench_association_state_queries(&mut self) -> Result<()> {
        let Some(inbox_id) = self.rand_inbox_id.clone() else {
            bail!("No inbox_id available for association state queries.");
        };

        // Read from cache (may or may not exist)
        bench!(self, read_from_cache(&inbox_id, 1))?;

        // Batch read from cache
        let identifiers = vec![(inbox_id.clone(), 1i64)];
        bench!(self, batch_read_from_cache(identifiers.clone()))?;

        Ok(())
    }

    fn bench_identity_update_queries(&mut self) -> Result<()> {
        let Some(inbox_id) = self.rand_inbox_id.clone() else {
            bail!("No inbox_id available for identity update queries.");
        };

        // Get identity updates with various filters
        bench!(self, get_identity_updates(&inbox_id, None, None))?;
        bench!(self, get_identity_updates(&inbox_id, Some(0), None))?;
        bench!(self, get_identity_updates(&inbox_id, None, Some(100)))?;

        // Get latest sequence ID for inbox
        bench!(self, get_latest_sequence_id_for_inbox(&inbox_id))?;

        // Get latest sequence IDs for multiple inboxes
        let inbox_ids = vec![inbox_id.as_str()];
        bench!(self, get_latest_sequence_id(&inbox_ids))?;

        Ok(())
    }

    fn bench_group_intent_queries(&mut self) -> Result<()> {
        let Some(group) = self.rand_group.clone() else {
            bail!("No group to run group intent queries on.");
        };

        // Find group intents with various filters
        bench!(self, find_group_intents(&group.id, None, None))?;
        bench!(
            self,
            find_group_intents(&group.id, Some(vec![IntentState::ToPublish]), None)
        )?;
        bench!(
            self,
            find_group_intents(
                &group.id,
                Some(vec![IntentState::ToPublish, IntentState::Published]),
                None
            )
        )?;
        bench!(
            self,
            find_group_intents(&group.id, None, Some(vec![IntentKind::SendMessage]))
        )?;

        // Find intent by payload hash (with a dummy hash that likely doesn't exist)
        let dummy_hash = vec![0u8; 32];
        bench!(self, find_group_intent_by_payload_hash(&dummy_hash))?;

        Ok(())
    }

    fn bench_refresh_state_queries(&mut self) -> Result<()> {
        let Some(group) = self.rand_group.clone() else {
            bail!("No group to run refresh state queries on.");
        };

        // Get refresh state
        bench!(
            self,
            get_refresh_state(&group.id, EntityKind::ApplicationMessage, 0)
        )?;

        // Get last cursor for originators
        bench!(
            self,
            get_last_cursor_for_originators(&group.id, EntityKind::ApplicationMessage, &[0, 10])
        )?;

        // Get last cursor for IDs
        let ids = vec![group.id.clone()];
        let entities = vec![EntityKind::ApplicationMessage, EntityKind::CommitMessage];
        bench!(self, get_last_cursor_for_ids(&ids, &entities))?;

        // Update cursor (this is idempotent with same/lower values)
        bench!(
            self,
            update_cursor(
                group.id.clone(),
                EntityKind::ApplicationMessage,
                Cursor {
                    sequence_id: 0,
                    originator_id: 0
                }
            )
        )?;

        // Latest cursor for ID
        bench!(self, latest_cursor_for_id(&group.id, &entities, None))?;

        // Get remote log cursors
        let conv_ids = vec![&group.id];
        bench!(self, get_remote_log_cursors(&conv_ids))?;

        Ok(())
    }

    fn bench_key_package_history_queries(&mut self) -> Result<()> {
        // Find key package history entries before a high ID (to get all)
        bench!(self, find_key_package_history_entries_before_id(i32::MAX))?;

        // Get expired key packages
        bench!(self, get_expired_key_packages())?;

        Ok(())
    }

    fn bench_conversation_list_queries(&mut self) -> Result<()> {
        // Fetch conversation list with default args
        bench!(self, fetch_conversation_list(GroupQueryArgs::default()))?;

        // Fetch conversation list with filters
        bench!(
            self,
            fetch_conversation_list(GroupQueryArgs {
                conversation_type: Some(ConversationType::Group),
                ..Default::default()
            })
        )?;

        bench!(
            self,
            fetch_conversation_list(GroupQueryArgs {
                conversation_type: Some(ConversationType::Dm),
                ..Default::default()
            })
        )?;

        bench!(
            self,
            fetch_conversation_list(GroupQueryArgs {
                limit: Some(10),
                ..Default::default()
            })
        )?;

        Ok(())
    }

    fn bench_commit_log_queries(&mut self) -> Result<()> {
        let Some(group) = self.rand_group.clone() else {
            bail!("No group to run commit log queries on.");
        };

        // Local commit log queries
        bench!(self, get_group_logs(&group.id))?;
        bench!(
            self,
            get_local_commit_log_after_cursor(&group.id, 0, LocalCommitLogOrder::AscendingByRowid)
        )?;
        bench!(self, get_latest_log_for_group(&group.id))?;
        bench!(self, get_local_commit_log_cursor(&group.id))?;

        // Remote commit log queries
        bench!(self, get_latest_remote_log_for_group(&group.id))?;
        bench!(
            self,
            get_remote_commit_log_after_cursor(
                &group.id,
                0,
                RemoteCommitLogOrder::AscendingByRowid
            )
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use xmtp_mls::tester;

    use crate::tasks::DbBencher;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_bench_works() {
        tester!(alix);

        let mut bencher = DbBencher::new(alix.context.store().clone())?;
        let result = bencher.bench()?;
        assert!(result.iter().all(|r| r.is_ok()));
    }
}
