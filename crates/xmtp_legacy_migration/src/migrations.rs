//! Pinned legacy migrations. See the package README for their source.
use crate::MigrationError;
use diesel::{
    connection::SimpleConnection,
    prelude::*,
    sql_types::{BigInt, Text},
};

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "20240506192337",
        include_str!("../migrations/2024-05-06-192337_openmls_storage/up.sql"),
    ),
    (
        "20240511004236",
        include_str!("../migrations/2024-05-11-004236_cache_association_state/up.sql"),
    ),
    (
        "20240515145138",
        include_str!("../migrations/2024-05-15-145138_new_schema/up.sql"),
    ),
    (
        "20240614220622",
        include_str!("../migrations/2024-06-14-220622_welcome_id_column/up.sql"),
    ),
    (
        "20240807213816",
        include_str!("../migrations/2024-08-07-213816_create-private-preference-store/up.sql"),
    ),
    (
        "20240822044745",
        include_str!("../migrations/2024-08-22-044745_add_staged_commit/up.sql"),
    ),
    (
        "20240909231735",
        include_str!("../migrations/2024-09-09-231735_create_dm_inbox_id/up.sql"),
    ),
    (
        "20240916221459",
        include_str!("../migrations/2024-09-16-221459_key_package_history/up.sql"),
    ),
    (
        "20240918185314",
        include_str!("../migrations/2024-09-18-185314_clear_association_states_cache/up.sql"),
    ),
    (
        "20241003004750",
        include_str!("../migrations/2024-10-03-004750_add_rotated_at_ns/up.sql"),
    ),
    (
        "20241105191238",
        include_str!("../migrations/2024-11-05-191238_cache_wallet_addresses/up.sql"),
    ),
    (
        "20241113145830",
        include_str!("../migrations/2024-11-13-145830_add_conversation_type_remove_purpose/up.sql"),
    ),
    (
        "20241205185829",
        include_str!("../migrations/2024-12-05-185829_create_user_preferences/up.sql"),
    ),
    (
        "20241206212729",
        include_str!("../migrations/2024-12-06-212729_make_hmac_required/up.sql"),
    ),
    (
        "20241211183736",
        include_str!(
            "../migrations/2024-12-11-183736_recreate_user_preferences_with_primary_key/up.sql"
        ),
    ),
    (
        "20241218175338",
        include_str!("../migrations/2024-12-18-175338_messages_content_type/up.sql"),
    ),
    (
        "20241220143210",
        include_str!("../migrations/2024-12-20-143210_create_conversation_list_view/up.sql"),
    ),
    (
        "20241220214747",
        include_str!("../migrations/2024-12-20-214747_add_dm_id/up.sql"),
    ),
    (
        "20250103002434",
        include_str!("../migrations/2025-01-03-002434_create_group_message_parent_id/up.sql"),
    ),
    (
        "20250116143131",
        include_str!("../migrations/2025-01-16-143131_add_message_expiration_to_groups/up.sql"),
    ),
    (
        "20250116200246",
        include_str!(
            "../migrations/2025-01-16-200246_conversation_list_filters_hardcoded_readable_types/up.sql"
        ),
    ),
    (
        "20250219210727",
        include_str!("../migrations/2025-02-19-210727_update_identity/up.sql"),
    ),
    (
        "20250303072233",
        include_str!("../migrations/2025-03-03-072233_paused_for_version/up.sql"),
    ),
    (
        "20250305045206",
        include_str!("../migrations/2025-03-05-045206_add_should_push_to_messages/up.sql"),
    ),
    (
        "20250317193321",
        include_str!("../migrations/2025-03-17-193321_update_dm_trigger/up.sql"),
    ),
    (
        "20250401185622",
        include_str!("../migrations/2025-04-01-185622_add_sync_cursor/up.sql"),
    ),
    (
        "20250422121314",
        include_str!("../migrations/2025-04-22-121314_groups_add_fork_status/up.sql"),
    ),
    (
        "20250509144408",
        include_str!("../migrations/2025-05-09-144408_add_seq_id_and_originator_node_id/up.sql"),
    ),
    (
        "20250513125624",
        include_str!("../migrations/2025-05-13-125624_create_icebox/up.sql"),
    ),
    (
        "20250520195531",
        include_str!("../migrations/2025-05-20-195531_create_stats_table/up.sql"),
    ),
    (
        "20250527201031",
        include_str!("../migrations/2025-05-27-201031_add_delete_at_and_rotate_in_to_keys/up.sql"),
    ),
    (
        "20250530232319",
        include_str!("../migrations/2025-05-30-232319_Add post_quantum_public_key/up.sql"),
    ),
    (
        "20250606161143",
        include_str!("../migrations/2025-06-06-161143_add_level_to_event/up.sql"),
    ),
    (
        "20250614004145",
        include_str!("../migrations/2025-06-14-004145_create_commit_log/up.sql"),
    ),
    (
        "20250708010431",
        include_str!("../migrations/2025-07-08-010431_modify_commit_log/up.sql"),
    ),
    (
        "20250715231627",
        include_str!("../migrations/2025-07-15-231627_add_should_publish_commit_log/up.sql"),
    ),
    (
        "20250717111748",
        include_str!("../migrations/2025-07-17-111748_add_delete_at_ns_to_group_messages/up.sql"),
    ),
    (
        "20250722185838",
        include_str!("../migrations/2025-07-22-185838_add_commit_log_public_key/up.sql"),
    ),
    (
        "20250807025914",
        include_str!("../migrations/2025-08-07-025914_add_commit_log_state/up.sql"),
    ),
    (
        "20250811185550",
        include_str!("../migrations/2025-08-11-185550_add_sequence_id_to_intents/up.sql"),
    ),
    (
        "20250812223606",
        include_str!(
            "../migrations/2025-08-12-223606_add_is_commit_log_forked_to_conversation_list/up.sql"
        ),
    ),
    (
        "20250819141841",
        include_str!("../migrations/2025-08-19-141841_originator_id_groups/up.sql"),
    ),
    (
        "20250820174800",
        include_str!("../migrations/2025-08-20-174800_d14n_originator_identity_updates/up.sql"),
    ),
    (
        "20250820175213",
        include_str!("../migrations/2025-08-20-175213_d14n_originator_refresh_state/up.sql"),
    ),
    (
        "20250820182831",
        include_str!("../migrations/2025-08-20-182831_d14n_originator_id_group_messages/up.sql"),
    ),
    (
        "20250825153518",
        include_str!("../migrations/2025-08-25-153518_add_group_has_pending_remove_members/up.sql"),
    ),
    (
        "20250828000637",
        include_str!("../migrations/2025-08-28-000637_create_readd_status/up.sql"),
    ),
    (
        "20250912232253",
        include_str!(
            "../migrations/2025-09-12-232253_optimize_dm_deduplication_performance/up.sql"
        ),
    ),
    (
        "20250916222143",
        include_str!("../migrations/2025-09-16-222143_add_pending_leave_members_table/up.sql"),
    ),
    (
        "20251007180046",
        include_str!("../migrations/2025-10-07-180046_create_tasks/up.sql"),
    ),
    (
        "20251008154142",
        include_str!("../migrations/2025-10-08-154142_group_intents_originator_id/up.sql"),
    ),
    (
        "202510281759200000",
        include_str!(
            "../migrations/2025-10-28-175920-0000_update_conversation_list_for_unknown_content_types/up.sql"
        ),
    ),
    (
        "202511141850540000",
        include_str!("../migrations/2025-11-14-185054-0000_add_dm_id_index/up.sql"),
    ),
    (
        "20251115232503",
        include_str!("../migrations/2025-11-15-232503_add_inserted_at_ns_to_group_messages/up.sql"),
    ),
    (
        "202511252132230000",
        include_str!(
            "../migrations/2025-11-25-213223-0000_make_icebox_depending_fields_non_null/up.sql"
        ),
    ),
    (
        "202512010000000000",
        include_str!("../migrations/2025-12-01-000000-0000_delete_message/up.sql"),
    ),
    (
        "202512081602150000",
        include_str!("../migrations/2025-12-08-160215-0000_drop_events_table/up.sql"),
    ),
    (
        "202512191539560000",
        include_str!("../migrations/2025-12-19-153956-0000_add_dm_group_updates_migrated/up.sql"),
    ),
    (
        "202601091523960000",
        include_str!(
            "../migrations/2026-01-09-152396-0000_add_should_push_to_group_messages/up.sql"
        ),
    ),
    (
        "202601271910000000",
        include_str!(
            "../migrations/2026-01-27-191000-0000_add_attempts_and_state_to_processed_device_sync_messages/up.sql"
        ),
    ),
    (
        "202602042033570000",
        include_str!("../migrations/2026-02-04-203357-0000_d14n_migration_cutover/up.sql"),
    ),
    (
        "202602050125540000",
        include_str!("../migrations/2026-02-05-012554-0000_add_missing_indexes/up.sql"),
    ),
    (
        "202603301200000000",
        include_str!(
            "../migrations/2026-03-30-120000-0000_add_registration_cursor_to_identity/up.sql"
        ),
    ),
    (
        "202606100000000000",
        include_str!(
            "../migrations/2026-06-10-000000-0000_add_idempotency_key_to_group_messages/up.sql"
        ),
    ),
];

#[derive(QueryableByName)]
struct Version {
    #[diesel(sql_type = Text)]
    version: String,
}

#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

// The longest supported legacy migration marker contains 18 ASCII bytes.
const MAX_VERSION_BYTES: i64 = 18;

/// Check SQL bounds before loading caller-controlled version strings.
fn version_history(conn: &mut SqliteConnection) -> Result<Vec<Version>, MigrationError> {
    let rows = diesel::sql_query(
        "SELECT count(*) AS count FROM (SELECT 1 FROM __diesel_schema_migrations LIMIT ?)",
    )
    .bind::<BigInt, _>((MIGRATIONS.len() + 1) as i64)
    .get_result::<Count>(conn)
    .map_err(MigrationError::migration)?;
    if rows.count == 0 || rows.count > MIGRATIONS.len() as i64 {
        return Err(MigrationError::UnsupportedSchema);
    }
    let invalid = diesel::sql_query("SELECT EXISTS(SELECT 1 FROM __diesel_schema_migrations WHERE typeof(version) != 'text' OR length(CAST(version AS BLOB)) > ?) AS count")
        .bind::<BigInt, _>(MAX_VERSION_BYTES)
        .get_result::<Count>(conn).map_err(MigrationError::migration)?;
    if invalid.count != 0 {
        return Err(MigrationError::UnsupportedSchema);
    }
    diesel::sql_query("SELECT version FROM __diesel_schema_migrations ORDER BY version LIMIT ?")
        .bind::<BigInt, _>(MIGRATIONS.len() as i64)
        .load::<Version>(conn)
        .map_err(MigrationError::migration)
}

/// Rejects unknown and non-prefix histories before applying a known suffix.
pub(crate) fn validate(conn: &mut SqliteConnection) -> Result<usize, MigrationError> {
    if diesel::sql_query("SELECT count(*) AS count FROM sqlite_master WHERE type='table' AND name='__diesel_schema_migrations'")
        .get_result::<Count>(conn).map_err(MigrationError::migration)?.count != 1 {
        return Err(MigrationError::UnsupportedSchema);
    }
    let versions = version_history(conn)?;
    if versions.is_empty()
        || versions.len() > MIGRATIONS.len()
        || versions
            .iter()
            .zip(MIGRATIONS)
            .any(|(actual, (expected, _))| actual.version != *expected)
    {
        return Err(MigrationError::UnsupportedSchema);
    }
    Ok(versions.len())
}

pub(crate) fn apply(
    conn: &mut SqliteConnection,
    mut check_cancelled: impl FnMut() -> Result<(), MigrationError>,
) -> Result<(), MigrationError> {
    check_cancelled()?;
    let applied = validate(conn)?;
    for (version, sql) in &MIGRATIONS[applied..] {
        check_cancelled()?;
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            conn.batch_execute(sql)?;
            diesel::sql_query("INSERT INTO __diesel_schema_migrations(version) VALUES (?)")
                .bind::<Text, _>(version)
                .execute(conn)?;
            Ok(())
        })
        .map_err(MigrationError::migration)?;
    }
    check_cancelled()
}

#[cfg(test)]
mod tests {
    use super::*;
    // verifies: MIG-003
    #[xmtp_common::test(unwrap_try = true)]
    fn history_row_and_value_limits_precede_loading() {
        let mut conn = SqliteConnection::establish(":memory:")?;
        conn.batch_execute("CREATE TABLE __diesel_schema_migrations(version TEXT)")?;
        for sql in [
            "INSERT INTO __diesel_schema_migrations VALUES (CAST(zeroblob(1048576) AS TEXT))",
            "INSERT INTO __diesel_schema_migrations VALUES ('xxxxxxxxxxxxxxxxxxx')",
            "INSERT INTO __diesel_schema_migrations VALUES ('éééééééééééééééééé')",
            "WITH RECURSIVE rows(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM rows WHERE n<100000) INSERT INTO __diesel_schema_migrations SELECT printf('%018d', n) FROM rows",
            "INSERT INTO __diesel_schema_migrations VALUES (x'32303234')",
            "INSERT INTO __diesel_schema_migrations VALUES (NULL)",
            "SELECT 1",
        ] {
            conn.batch_execute("DELETE FROM __diesel_schema_migrations")?;
            conn.batch_execute(sql)?;
            assert!(
                matches!(
                    version_history(&mut conn),
                    Err(MigrationError::UnsupportedSchema)
                ),
                "history admitted before loading: {sql}"
            );
        }
        for (version, _) in MIGRATIONS {
            diesel::sql_query("INSERT INTO __diesel_schema_migrations VALUES (?)")
                .bind::<Text, _>(version)
                .execute(&mut conn)?;
        }
        let versions = version_history(&mut conn)?;
        assert_eq!(versions.len(), 64);
        assert_eq!(versions.last().unwrap().version, "202606100000000000");
    }

    // verifies: MIG-002
    #[xmtp_common::test(unwrap_try = true)]
    fn every_known_schema_prefix_reaches_the_pinned_endpoint() {
        const PINNED_ENDPOINT: usize = 64;
        assert_eq!(MIGRATIONS.len(), PINNED_ENDPOINT);
        for prefix in 1..=PINNED_ENDPOINT {
            let mut conn = SqliteConnection::establish(":memory:")?;
            conn.batch_execute("CREATE TABLE __diesel_schema_migrations(version TEXT PRIMARY KEY NOT NULL, run_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)")?;
            for (version, sql) in &MIGRATIONS[..prefix] {
                conn.batch_execute(sql)?;
                diesel::sql_query("INSERT INTO __diesel_schema_migrations(version) VALUES (?)")
                    .bind::<Text, _>(version)
                    .execute(&mut conn)?;
            }
            apply(&mut conn, || Ok(()))?;
            assert_eq!(validate(&mut conn)?, PINNED_ENDPOINT, "prefix {prefix}");
        }
    }
}
