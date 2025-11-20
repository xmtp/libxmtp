use anyhow::{Result, bail};
use std::collections::HashMap;
use xmtp_db::{
    EncryptedMessageStore, NativeDb,
    consent_record::{ConsentState, ConsentType, QueryConsentRecord, StoredConsentRecord},
    group::{ConversationType, GroupQueryArgs, QueryGroup, StoredGroup},
    group_message::{MsgQueryArgs, QueryGroupMessage, RelationQuery},
    proto::types::{Cursor, GlobalCursor},
};

macro_rules! bench {
    ($self:ident, $fn:ident($($args:expr),*)) => {{
        let key = stringify!($fn($($args),*));
        let store = $self.store.clone();
        let result = $self.bench_with_key(key, || store.db().$fn($($args),*));
        result
    }};
}

pub struct DbBencher {
    rand_dm: Option<StoredGroup>,
    rand_group: Option<StoredGroup>,
    measurements: HashMap<String, f32>,
    store: EncryptedMessageStore<NativeDb>,
}

impl DbBencher {
    pub fn new(store: EncryptedMessageStore<NativeDb>) -> Result<Self> {
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

        Ok(Self {
            rand_dm: dms.pop(),
            rand_group: groups.pop(),
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

    pub fn bench(&mut self) -> Result<()> {
        if let Err(err) = self.bench_group_queries() {
            tracing::error!("bench_group_queries: {err:?}");
        };
        if let Err(err) = self.bench_consent_queries() {
            tracing::error!("bench_consent_queries: {err:?}");
        };
        if let Err(err) = self.bench_message_queries() {
            tracing::error!("bench_message_queries: {err:?}");
        };

        self.print_results();

        Ok(())
    }

    fn print_results(&self) {
        // Sort measurements by execution time (greatest to least)
        let mut sorted_measurements: Vec<_> = self.measurements.iter().collect();
        sorted_measurements.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());

        println!("\n{}", "=".repeat(80));
        println!("{:^80}", "Database Benchmark Results");
        println!("{}", "=".repeat(80));
        println!("{:<50} {:>15} {:>10}", "Query", "Time (ms)", "Relative");
        println!("{}", "-".repeat(80));

        let max_time = sorted_measurements.first().map(|(_, t)| **t).unwrap_or(1.0);

        for (query, time_ms) in sorted_measurements.into_iter() {
            let relative = time_ms / max_time;
            let bar_length = (relative * 30.0) as usize;
            let bar = "█".repeat(bar_length);

            println!(
                "{:<50} {:>12.3} ms {:>7.1}% {}",
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

    pub fn bench_consent_queries(&mut self) -> Result<()> {
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
            insert_or_replace_consent_records(&[new_consent.clone()])
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
}
