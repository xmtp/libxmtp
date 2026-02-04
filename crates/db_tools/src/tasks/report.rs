use anyhow::{Result, bail};
use std::{cmp::Ordering, collections::HashMap};
use xmtp_db::diesel;
use xmtp_db::{
    ConnectionError, ConnectionExt, XmtpDb,
    association_state::QueryAssociationStateCache,
    consent_record::{ConsentState, ConsentType, QueryConsentRecord, StoredConsentRecord},
    conversation_list::QueryConversationList,
    diesel::{
        Connection, ExpressionMethods, QueryDsl, QueryableByName, RunQueryDsl, sql_types::BigInt,
    },
    group::{ConversationType, GroupQueryArgs, QueryGroup, StoredGroup},
    group_intent::{IntentKind, IntentState, QueryGroupIntent},
    group_message::{MsgQueryArgs, QueryGroupMessage, RelationQuery},
    icebox::QueryIcebox,
    identity::QueryIdentity,
    identity_update::QueryIdentityUpdates,
    key_package_history::QueryKeyPackageHistory,
    local_commit_log::{LocalCommitLogOrder, QueryLocalCommitLog},
    message_deletion::QueryMessageDeletion,
    pending_remove::QueryPendingRemove,
    prelude::{QueryDms, QueryGroupVersion},
    processed_device_sync_messages::QueryDeviceSyncMessages,
    proto::types::{Cursor, GlobalCursor},
    readd_status::QueryReaddStatus,
    refresh_state::{EntityKind, QueryRefreshState},
    remote_commit_log::{QueryRemoteCommitLog, RemoteCommitLogOrder},
    schema::{
        association_state, consent_records, group_intents, group_messages, groups, icebox,
        icebox_dependencies, identity, identity_cache, identity_updates, key_package_history,
        local_commit_log, message_deletions, openmls_key_store, openmls_key_value, pending_remove,
        processed_device_sync_messages, readd_status, refresh_state, remote_commit_log, tasks,
        user_preferences,
    },
    tasks::QueryTasks,
};

#[derive(QueryableByName)]
struct CountResult {
    #[diesel(sql_type = BigInt)]
    cnt: i64,
}

/// Helper for rendering formatted tables
struct TablePrinter {
    title: String,
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
    footer: Option<Vec<String>>,
}

struct Column {
    header: String,
    width: usize,
    align_right: bool,
}

impl TablePrinter {
    fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            columns: Vec::new(),
            rows: Vec::new(),
            footer: None,
        }
    }

    fn column(mut self, header: impl Into<String>, min_width: usize, align_right: bool) -> Self {
        self.columns.push(Column {
            header: header.into(),
            width: min_width,
            align_right,
        });
        self
    }

    fn row(mut self, values: Vec<String>) -> Self {
        self.rows.push(values);
        self
    }

    fn footer(mut self, values: Vec<String>) -> Self {
        self.footer = Some(values);
        self
    }

    fn print(mut self) {
        // Calculate column widths based on content
        for (i, col) in self.columns.iter_mut().enumerate() {
            col.width = col.width.max(col.header.len());
            for row in &self.rows {
                if let Some(val) = row.get(i) {
                    col.width = col.width.max(val.len());
                }
            }
            if let Some(footer) = &self.footer {
                if let Some(val) = footer.get(i) {
                    col.width = col.width.max(val.len());
                }
            }
        }

        let total_width: usize =
            self.columns.iter().map(|c| c.width).sum::<usize>() + (self.columns.len() - 1) * 2;

        // Print header
        println!("\n{}", "=".repeat(total_width));
        println!("{:^width$}", self.title, width = total_width);
        println!("{}", "=".repeat(total_width));

        // Print column headers
        let header_row: Vec<String> = self
            .columns
            .iter()
            .map(|c| {
                if c.align_right {
                    format!("{:>width$}", c.header, width = c.width)
                } else {
                    format!("{:<width$}", c.header, width = c.width)
                }
            })
            .collect();
        println!("{}", header_row.join("  "));
        println!("{}", "-".repeat(total_width));

        // Print rows
        for row in &self.rows {
            let formatted: Vec<String> = self
                .columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let val = row.get(i).map(|s| s.as_str()).unwrap_or("");
                    if col.align_right {
                        format!("{:>width$}", val, width = col.width)
                    } else {
                        format!("{:<width$}", val, width = col.width)
                    }
                })
                .collect();
            println!("{}", formatted.join("  "));
        }

        // Print footer if present
        if let Some(footer) = &self.footer {
            println!("{}", "-".repeat(total_width));
            let formatted: Vec<String> = self
                .columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let val = footer.get(i).map(|s| s.as_str()).unwrap_or("");
                    if col.align_right {
                        format!("{:>width$}", val, width = col.width)
                    } else {
                        format!("{:<width$}", val, width = col.width)
                    }
                })
                .collect();
            println!("{}", formatted.join("  "));
        }

        println!("{}", "=".repeat(total_width));
    }
}

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
    rand_installation_id: Option<Vec<u8>>,
    measurements: HashMap<String, f32>,
    table_counts: HashMap<String, i64>,
    intent_state_counts: HashMap<String, (i64, i64, i64)>, // (count, with_commit_count, groups_count)
    store: Db,
}

impl<Db> DbBencher<Db>
where
    Db: XmtpDb + Clone,
    <Db as XmtpDb>::Connection: ConnectionExt,
{
    pub fn new(store: Db) -> Result<Self> {
        let mut dms = store.db().find_groups(GroupQueryArgs {
            conversation_type: Some(ConversationType::Dm),
            ..Default::default()
        })?;
        let mut groups = store.db().find_groups(GroupQueryArgs {
            conversation_type: Some(ConversationType::Group),
            ..Default::default()
        })?;

        tracing::info!("Found {} Groups", groups.len());
        tracing::info!("Found {} DMs", dms.len());

        // Try to get a random inbox_id from identity updates or association state
        let rand_inbox_id = groups.first().map(|g| g.added_by_inbox_id.clone());

        // Generate a random installation ID for benchmarks that need it
        let rand_installation_id = Some(vec![0u8; 32]);

        Ok(Self {
            rand_dm: dms.pop(),
            rand_group: groups.pop(),
            rand_inbox_id,
            rand_installation_id,
            store,
            measurements: HashMap::default(),
            table_counts: HashMap::default(),
            intent_state_counts: HashMap::default(),
        })
    }

    fn count_table_records(&mut self) -> Result<()> {
        use xmtp_db::diesel::dsl::count_star;
        use xmtp_db::diesel::sql_query;

        self.store.conn().raw_query_read(|conn| {
            // Count group_intents by state
            let intent_states = [
                ("ToPublish", IntentState::ToPublish as i32),
                ("Published", IntentState::Published as i32),
                ("Committed", IntentState::Committed as i32),
                ("Error", IntentState::Error as i32),
                ("Processed", IntentState::Processed as i32),
            ];

            for (state_name, state_value) in intent_states {
                let count = group_intents::table
                    .filter(group_intents::dsl::state.eq(state_value))
                    .select(count_star())
                    .first::<i64>(conn)
                    .unwrap_or(0);

                let with_commit_count = group_intents::table
                    .filter(group_intents::dsl::state.eq(state_value))
                    .filter(group_intents::dsl::staged_commit.is_not_null())
                    .select(count_star())
                    .first::<i64>(conn)
                    .unwrap_or(0);

                // Count distinct groups with intents in this state
                let groups_count = sql_query(format!(
                    "SELECT COUNT(DISTINCT group_id) as cnt FROM group_intents WHERE state = {}",
                    state_value
                ))
                .get_result::<CountResult>(conn)
                .map(|r| r.cnt)
                .unwrap_or(0);

                self.intent_state_counts.insert(
                    state_name.to_string(),
                    (count, with_commit_count, groups_count),
                );
            }

            // Count records in each table
            let counts: Vec<(&str, i64)> = vec![
                (
                    "association_state",
                    association_state::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "consent_records",
                    consent_records::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "group_intents",
                    group_intents::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "group_messages",
                    group_messages::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "groups",
                    groups::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "icebox",
                    icebox::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "icebox_dependencies",
                    icebox_dependencies::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "identity",
                    identity::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "identity_cache",
                    identity_cache::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "identity_updates",
                    identity_updates::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "key_package_history",
                    key_package_history::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "local_commit_log",
                    local_commit_log::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "message_deletions",
                    message_deletions::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "openmls_key_store",
                    openmls_key_store::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "openmls_key_value",
                    openmls_key_value::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "pending_remove",
                    pending_remove::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "processed_device_sync_messages",
                    processed_device_sync_messages::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "readd_status",
                    readd_status::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "refresh_state",
                    refresh_state::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "remote_commit_log",
                    remote_commit_log::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "tasks",
                    tasks::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
                (
                    "user_preferences",
                    user_preferences::table
                        .select(count_star())
                        .first::<i64>(conn)
                        .unwrap_or(0),
                ),
            ];

            for (table_name, count) in counts {
                self.table_counts.insert(table_name.to_string(), count);
            }

            Ok::<_, xmtp_db::diesel::result::Error>(())
        })?;

        Ok(())
    }

    fn group_or_dm(&self) -> Option<StoredGroup> {
        self.rand_dm.as_ref().or(self.rand_group.as_ref()).cloned()
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
        // Count table records before benchmarking
        self.count_table_records()?;

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
                results.push(self.bench_dm_queries());
                results.push(self.bench_message_deletion_queries());
                results.push(self.bench_device_sync_queries());
                results.push(self.bench_task_queries());
                results.push(self.bench_icebox_queries());
                results.push(self.bench_readd_status_queries());
                results.push(self.bench_pending_remove_queries());
                results.push(self.bench_identity_queries());
                results.push(self.bench_group_version_queries());

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
        // Print table record counts first
        self.print_table_counts();

        // Sort measurements by execution time (greatest to least)
        let mut sorted_measurements: Vec<_> = self.measurements.iter().collect();
        // Send divide-by-zeroes to the bottom
        sorted_measurements.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(Ordering::Less));

        // Normalize query names by collapsing whitespace/newlines into single spaces
        let normalized_measurements: Vec<_> = sorted_measurements
            .into_iter()
            .map(|(q, t)| {
                let normalized: String = q.split_whitespace().collect::<Vec<_>>().join(" ");
                (normalized, *t)
            })
            .collect();

        // Calculate the width needed for the query column based on longest query name
        let query_width = normalized_measurements
            .iter()
            .map(|(q, _)| q.len())
            .max()
            .unwrap_or(5)
            .max(5); // At least "Query" width

        let total_width = query_width + 12 + 10 + 25 + 6; // query + time + relative + bar + spacing

        println!("\n{}", "=".repeat(total_width));
        println!(
            "{:^width$}",
            "Database Benchmark Results",
            width = total_width
        );
        println!("{}", "=".repeat(total_width));
        println!(
            "{:<query_width$} {:>12} {:>10}",
            "Query",
            "Time (ms)",
            "Relative",
            query_width = query_width
        );
        println!("{}", "-".repeat(total_width));

        let max_time = normalized_measurements
            .first()
            .map(|(_, t)| *t)
            .unwrap_or(1.0);

        for (query, time_ms) in normalized_measurements.iter() {
            let relative = time_ms / max_time;
            let bar_length = (relative * 20.0) as usize;
            let bar = "█".repeat(bar_length);

            println!(
                "{:<query_width$} {:>12.3} {:>9.1}% {}",
                query,
                time_ms,
                relative * 100.0,
                bar,
                query_width = query_width
            );
        }

        println!("{}", "=".repeat(total_width));
        println!(
            "Total queries benchmarked: {} | Average time: {:.3} ms\n",
            self.measurements.len(),
            self.measurements.values().sum::<f32>() / self.measurements.len() as f32
        );
    }

    fn print_table_counts(&self) {
        let mut sorted_counts: Vec<_> = self.table_counts.iter().collect();
        sorted_counts.sort_by(|a, b| b.1.cmp(a.1));

        let total_records: i64 = sorted_counts.iter().map(|(_, c)| *c).sum();

        let mut table = TablePrinter::new("Table Record Counts")
            .column("Table Name", 10, false)
            .column("Record Count", 12, true);

        for (table_name, count) in sorted_counts {
            table = table.row(vec![table_name.to_string(), format_count(*count)]);
        }

        table
            .footer(vec!["TOTAL".to_string(), format_count(total_records)])
            .print();

        self.print_intent_breakdown();
    }

    fn print_intent_breakdown(&self) {
        let total_intents = self.table_counts.get("group_intents").copied().unwrap_or(0);

        if total_intents == 0 {
            return;
        }

        let mut state_counts: Vec<_> = self.intent_state_counts.iter().collect();
        state_counts.sort_by(|a, b| b.1.0.cmp(&a.1.0));

        let mut table = TablePrinter::new("Group Intents Breakdown")
            .column("State", 10, false)
            .column("Count", 10, true)
            .column("Percent", 8, true)
            .column("With Commit", 12, true)
            .column("Commit %", 8, true)
            .column("Groups", 8, true);

        for (state, (count, with_commit, groups_count)) in state_counts {
            let percent = if total_intents > 0 {
                (*count as f64 / total_intents as f64) * 100.0
            } else {
                0.0
            };
            let commit_percent = if *count > 0 {
                (*with_commit as f64 / *count as f64) * 100.0
            } else {
                0.0
            };
            table = table.row(vec![
                state.to_string(),
                format_count(*count),
                format!("{:.1}%", percent),
                format_count(*with_commit),
                format!("{:.1}%", commit_percent),
                format_count(*groups_count),
            ]);
        }

        table.print();
    }

    pub fn bench_message_queries(&mut self) -> Result<()> {
        let Some(group) = self.group_or_dm() else {
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
        bench!(self, sync_group_messages_paged(0, 100))?;

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

            // Cursor-based query
            let cursor = Cursor::new(message.sequence_id as u64, message.originator_id as u32);
            bench!(self, get_group_message_by_cursor(&group.id, cursor))?;

            // Delivery status updates (non-destructive operations should be safe to benchmark)
            bench!(
                self,
                set_delivery_status_to_published(&message_id, 0, Cursor::new(0, 0u32), None)
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
        let Some(group) = self.group_or_dm() else {
            bail!("No groups to run group queries on.");
        };
        bench!(self, get_conversation_ids_for_remote_log_download())?;
        bench!(self, get_conversation_ids_for_remote_log_publish())?;
        bench!(self, get_conversation_ids_for_fork_check())?;
        bench!(self, get_conversation_ids_for_requesting_readds())?;
        bench!(self, get_conversation_ids_for_responding_readds())?;
        bench!(self, get_conversation_type(&group.id))?;
        bench!(self, find_groups(GroupQueryArgs::default()))?;
        bench!(self, find_groups_by_id_paged(GroupQueryArgs::default(), 0))?;
        bench!(self, all_sync_groups())?;
        bench!(self, primary_sync_group())?;
        bench!(self, find_group(&group.id))?;
        bench!(self, find_sync_group(&group.id))?;
        bench!(self, get_rotated_at_ns(group.id.clone()))?;
        bench!(self, update_rotated_at_ns(group.id.clone()))?;
        bench!(self, get_installations_time_checked(group.id.clone()))?;
        bench!(self, update_installations_time_checked(group.id.clone()))?;
        bench!(
            self,
            update_message_disappearing_from_ns(group.id.clone(), Some(1000))
        )?;
        bench!(
            self,
            update_message_disappearing_in_ns(group.id.clone(), Some(1000))
        )?;

        // Fork status queries
        bench!(self, get_group_commit_log_forked_status(&group.id))?;
        bench!(self, get_groups_have_pending_leave_request())?;

        // Check for duplicate DM
        bench!(self, has_duplicate_dm(&group.id))?;

        // Find group by sequence ID (using a dummy cursor)
        let cursor = Cursor::new(0, 0u32);
        bench!(self, find_group_by_sequence_id(cursor))?;

        Ok(())
    }

    fn bench_consent_queries(&mut self) -> Result<()> {
        let Some(group) = self.group_or_dm() else {
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
        if let Some(dm) = &self.rand_dm
            && let Some(dm_id) = dm.dm_id.clone()
        {
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

        // Count inbox updates
        bench!(self, count_inbox_updates(&inbox_ids))?;

        Ok(())
    }

    fn bench_group_intent_queries(&mut self) -> Result<()> {
        let Some(group) = self.group_or_dm() else {
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

        // Find dependant commits
        let payload_hashes: Vec<Vec<u8>> = vec![dummy_hash.clone()];
        bench!(self, find_dependant_commits(&payload_hashes))?;

        Ok(())
    }

    fn bench_refresh_state_queries(&mut self) -> Result<()> {
        let Some(group) = self.group_or_dm() else {
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
                Cursor::new(0, 0u32)
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
        let Some(group) = self.group_or_dm() else {
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

    fn bench_dm_queries(&mut self) -> Result<()> {
        let Some(dm) = self.rand_dm.clone() else {
            bail!("No DM available for DM queries.");
        };

        // Fetch stitched DM
        bench!(self, fetch_stitched(&dm.id))?;

        // Load other DMs stitched into this group
        bench!(self, other_dms(&dm.id))?;

        // Find active DM group (using dm_id if available)
        if let Some(dm_id) = &dm.dm_id {
            let dm_id_clone = dm_id.clone();
            bench!(self, find_active_dm_group(&dm_id_clone))?;
        }

        Ok(())
    }

    fn bench_message_deletion_queries(&mut self) -> Result<()> {
        let Some(group) = self.rand_group.clone() else {
            bail!("No group to run message deletion queries on.");
        };

        // Get group deletions
        bench!(self, get_group_deletions(&group.id))?;

        // Try to get a message ID for deletion queries
        let messages = self
            .store
            .db()
            .get_group_messages(&group.id, &MsgQueryArgs::default())?;

        if let Some(message) = messages.first() {
            let message_id = message.id.clone();

            // Get message deletion by ID
            bench!(self, get_message_deletion(&message_id))?;

            // Get deletion by deleted message ID
            bench!(self, get_deletion_by_deleted_message_id(&message_id))?;

            // Check if message is deleted
            bench!(self, is_message_deleted(&message_id))?;

            // Get deletions for multiple messages
            let message_ids = vec![message_id.clone()];
            bench!(self, get_deletions_for_messages(message_ids.clone()))?;
        }

        Ok(())
    }

    fn bench_device_sync_queries(&mut self) -> Result<()> {
        // Unprocessed sync group messages
        bench!(self, unprocessed_sync_group_messages())?;

        // Sync group messages paged (already covered but adding here for completeness)
        bench!(self, sync_group_messages_paged(0, 100))?;

        Ok(())
    }

    fn bench_task_queries(&mut self) -> Result<()> {
        // Get all tasks
        bench!(self, get_tasks())?;

        // Get next task
        bench!(self, get_next_task())?;

        Ok(())
    }

    fn bench_icebox_queries(&mut self) -> Result<()> {
        // Past dependents with empty cursors
        let empty_cursors: Vec<Cursor> = vec![];
        bench!(self, past_dependents(&empty_cursors))?;

        // Future dependents with empty cursors
        bench!(self, future_dependents(&empty_cursors))?;

        // Prune icebox
        bench!(self, prune_icebox())?;

        Ok(())
    }

    fn bench_readd_status_queries(&mut self) -> Result<()> {
        let Some(group) = self.rand_group.clone() else {
            bail!("No group to run readd status queries on.");
        };

        let Some(installation_id) = self.rand_installation_id.clone() else {
            bail!("No installation_id available for readd status queries.");
        };

        // Get readd status
        bench!(self, get_readd_status(&group.id, &installation_id))?;

        // Check if awaiting readd
        bench!(self, is_awaiting_readd(&group.id, &installation_id))?;

        // Get readds awaiting response
        bench!(
            self,
            get_readds_awaiting_response(&group.id, &installation_id)
        )?;

        Ok(())
    }

    fn bench_pending_remove_queries(&mut self) -> Result<()> {
        let Some(group) = self.rand_group.clone() else {
            bail!("No group to run pending remove queries on.");
        };

        // Get pending remove users
        bench!(self, get_pending_remove_users(&group.id))?;

        // Get user pending remove status
        let inbox_id = self.rand_inbox_id.clone().unwrap_or_default();
        bench!(self, get_user_pending_remove_status(&group.id, &inbox_id))?;

        Ok(())
    }

    fn bench_identity_queries(&mut self) -> Result<()> {
        // Check if identity needs rotation
        bench!(self, is_identity_needs_rotation())?;

        Ok(())
    }

    fn bench_group_version_queries(&mut self) -> Result<()> {
        let Some(group) = self.rand_group.clone() else {
            bail!("No group to run group version queries on.");
        };

        // Get group paused version
        bench!(self, get_group_paused_version(&group.id))?;

        Ok(())
    }
}

fn format_count(count: i64) -> String {
    // Format number with thousand separators
    let s = count.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use crate::tasks::DbBencher;
    use xmtp_mls::tester;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_bench_works() {
        tester!(alix, persistent_db);
        tester!(bo);

        alix.test_talk_in_new_group_with(&bo).await?;
        alix.test_talk_in_dm_with(&bo).await?;

        let mut bencher = DbBencher::new(alix.context.store().clone())?;
        let result = bencher.bench()?;
        assert!(result.iter().all(|r| r.is_ok()));
    }
}
