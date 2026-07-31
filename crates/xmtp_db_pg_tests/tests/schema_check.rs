//! Every `PgModel`'s column list, checked against the live Postgres catalog.
//!
//! This is the async track's substitute for `diesel::table!`. On the sync track
//! a column rename breaks the build, because `schema.rs` and the `Queryable`
//! derives have to agree. The async track has no schema module, so until now
//! nothing connected a model's fields to the actual table — a renamed or
//! retyped column would surface as a runtime decode error, in production, on
//! whichever query happened to touch it first.
//!
//! Checking against `information_schema` closes that gap for every model at
//! once, and does it in one place rather than relying on each query's own test
//! to happen to cover the affected column.

use sqlx::Row;
use xmtp_db::pg::PgModel;
use xmtp_db_pg_tests::fresh_db;

/// `(model name, table, columns)` for each `PgModel`.
///
/// Adding a model here is manual. A missing entry costs coverage, not
/// correctness — the check still passes, it just checks less — so if you add a
/// `#[derive(PgModel)]`, add it here too.
fn models() -> Vec<(&'static str, &'static str, &'static [&'static str])> {
    use xmtp_db::consent_record::StoredConsentRecord;
    use xmtp_db::conversation_list::ConversationListItem;
    use xmtp_db::group::{
        StoredGroup, StoredGroupCommitLogPublicKey, StoredGroupForRespondingReadds,
    };
    use xmtp_db::group_intent::StoredGroupIntent;
    use xmtp_db::icebox::{Icebox, IceboxDependency};
    use xmtp_db::group_message::StoredGroupMessage;
    use xmtp_db::identity_update::StoredIdentityUpdate;
    use xmtp_db::key_package_history::StoredKeyPackageHistoryEntry;
    use xmtp_db::local_commit_log::LocalCommitLog;
    use xmtp_db::message_deletion::StoredMessageDeletion;
    use xmtp_db::readd_status::ReaddStatus;
    use xmtp_db::refresh_state::RefreshState;
    use xmtp_db::remote_commit_log::RemoteCommitLog;
    use xmtp_db::tasks::Task;
    use xmtp_db::user_preferences::StoredUserPreferences;

    fn entry<M: PgModel>(
        name: &'static str,
    ) -> (&'static str, &'static str, &'static [&'static str]) {
        (name, M::TABLE, M::COLUMNS)
    }

    vec![
        entry::<StoredGroup>("StoredGroup"),
        // Projections of `groups`, so their column lists are checked too.
        entry::<StoredGroupCommitLogPublicKey>("StoredGroupCommitLogPublicKey"),
        entry::<StoredGroupForRespondingReadds>("StoredGroupForRespondingReadds"),
        entry::<StoredGroupMessage>("StoredGroupMessage"),
        entry::<StoredGroupIntent>("StoredGroupIntent"),
        // A view rather than a table; `information_schema.columns` covers both.
        entry::<ConversationListItem>("ConversationListItem"),
        entry::<StoredConsentRecord>("StoredConsentRecord"),
        entry::<StoredIdentityUpdate>("StoredIdentityUpdate"),
        entry::<StoredKeyPackageHistoryEntry>("StoredKeyPackageHistoryEntry"),
        entry::<StoredMessageDeletion>("StoredMessageDeletion"),
        entry::<LocalCommitLog>("LocalCommitLog"),
        entry::<RemoteCommitLog>("RemoteCommitLog"),
        entry::<ReaddStatus>("ReaddStatus"),
        entry::<RefreshState>("RefreshState"),
        entry::<Task>("Task"),
        entry::<StoredUserPreferences>("StoredUserPreferences"),
        entry::<Icebox>("Icebox"),
        entry::<IceboxDependency>("IceboxDependency"),
    ]
}

/// Every column a model declares must exist on its table.
///
/// Deliberately one-directional: a table may carry columns no model reads
/// (`group_messages.rowid` exists only as an insertion-order tie-break and is
/// invisible to the models by design), so extra columns are not a failure.
#[tokio::test]
async fn every_model_column_exists_in_the_database() {
    let db = fresh_db("schema_check").await;
    let mut conn = db.conn().await.unwrap();

    let mut problems: Vec<String> = Vec::new();

    for (model, table, columns) in models() {
        let rows = sqlx::query(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = $1",
        )
        .bind(table)
        .fetch_all(&mut *conn)
        .await
        .unwrap();

        let actual: Vec<String> = rows
            .iter()
            .map(|r| r.try_get::<String, _>(0).unwrap())
            .collect();

        if actual.is_empty() {
            problems.push(format!(
                "{model}: table `{table}` does not exist in the migrated schema"
            ));
            continue;
        }

        for column in columns {
            if !actual.iter().any(|a| a == column) {
                problems.push(format!(
                    "{model}: column `{table}.{column}` is declared by the model but not in the \
                     database (table has: {})",
                    actual.join(", ")
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "PgModel column lists disagree with the Postgres schema:\n  {}",
        problems.join("\n  ")
    );
}

/// A model's columns must be selectable together — this is what catches a *type*
/// change that a name-only check would miss, since the query is prepared against
/// the real table.
#[tokio::test]
async fn every_model_select_list_is_valid_sql() {
    let db = fresh_db("schema_select").await;
    let mut conn = db.conn().await.unwrap();

    for (model, table, columns) in models() {
        let sql = format!("SELECT {} FROM {table} WHERE false", columns.join(", "));
        sqlx::query(&sql)
            .fetch_all(&mut *conn)
            .await
            .unwrap_or_else(|e| panic!("{model}: select list is not valid against `{table}`: {e}"));
    }
}
