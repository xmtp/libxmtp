//! Compare stopped client databases without migrations or write access.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use xmtp_db::{
    diesel::{
        Connection, ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl,
        connection::SimpleConnection,
        dsl::sql,
        sql_types::{Binary, Bool, Nullable, Text},
        sqlite::SqliteConnection,
    },
    local_commit_log::LocalCommitLog,
    remote_commit_log::CommitResult,
};

use super::{BoundedOutput, LINE_BYTES, OUTPUT_LINES, read_json, safe_error, scalar};
use crate::ledger::{read_bounded, regular_size};

const CONFIG_BYTES: usize = 64 * 1024;
const HISTORY_ROWS: i64 = 4096;
const MAX_INSTALLATIONS: usize = 19;
const MAX_GROUPS: usize = 32;
const FLAG_ERROR_ROWS: usize = 4;
const AUTHENTICATOR_BYTES: usize = 64;

struct History {
    slot: u64,
    records: Vec<LocalCommitLog>,
    capped: bool,
    maybe_forked: bool,
    commit_log_forked: Option<bool>,
    fork_details: String,
}

#[derive(Default, Debug)]
struct Comparison {
    common_sequences: usize,
    mismatch_count: usize,
    mismatches: Vec<(i64, u64, u64)>,
    exclusions: usize,
}

pub(crate) fn inspect(path: &Path, group: Option<&str>) -> String {
    let mut output = BoundedOutput::new(OUTPUT_LINES, LINE_BYTES);
    if let Err(error) = inspect_inner(&mut output, path, group) {
        output.line(&format!(
            "INCONCLUSIVE: {}",
            safe_error(&format!("{error:#}"))
        ));
    }
    output.finish()
}

fn inspect_inner(output: &mut BoundedOutput, path: &Path, selector: Option<&str>) -> Result<()> {
    let root = path.canonicalize()?;
    let status = read_json(&root.join("status.json"))?;
    ensure!(
        status["phase"] == "stopped",
        "fork inspection requires stopped writers (status.phase must be stopped)"
    );
    let instances = status["installations"]
        .as_array()
        .context("saved installation snapshots are missing")?;
    ensure!(
        instances.len() <= MAX_INSTALLATIONS,
        "installation input cap reached"
    );
    if let Some(group) = selector {
        validate_group(group)?;
    }
    let mut groups = BTreeSet::new();
    for instance in instances {
        for group in instance["groups"].as_array().into_iter().flatten() {
            let id = group["group_id"].as_str().context("group id is missing")?;
            validate_group(id)?;
            if selector.is_none_or(|selector| selector == id) {
                groups.insert(id);
            }
            ensure!(groups.len() <= MAX_GROUPS, "group input cap reached");
        }
    }
    ensure!(!groups.is_empty(), "no matching saved groups");
    output.line(&format!("Stopped fork inspection seed={} round={} groups={}; history cap={HISTORY_ROWS} records per database/group", scalar(&status["seed"]), scalar(&status["round"]), groups.len()));
    output.line("Current state comes from the final checkpoint. Historical comparisons use read-only database records.");
    output.line("Only shared positive sequences are comparable; Welcome anchors and terminal removal records are excluded.");
    let mut any_inconclusive = false;
    let mut mismatch_count = 0;
    for group in groups {
        output.line(&format!("GROUP {group}"));
        let selected: Vec<_> = instances
            .iter()
            .filter_map(|instance| {
                instance["groups"]
                    .as_array()
                    .and_then(|groups| groups.iter().find(|entry| entry["group_id"] == group))
                    .map(|snapshot| (instance, snapshot))
            })
            .collect();
        let current = compare_current(&selected);
        any_inconclusive |= current.2 < 2;
        mismatch_count += usize::from(!current.0);
        output.line(&format!("  current active instance views={} state={} cursor={} (members and metadata compared without display)", current.2, if current.0 { "EQUAL" } else { "MISMATCH" }, if current.1 { "EQUAL" } else { "DIFFERENT" }));
        let mut histories = Vec::new();
        let mut seen_databases = BTreeSet::new();
        for (instance, snapshot) in &selected {
            let slot = instance["instance"]
                .as_u64()
                .context("instance slot is missing")?;
            ensure!(slot < MAX_INSTALLATIONS as u64, "invalid installation slot");
            let complete = checkpoint_complete(&instance["checkpoint"]);
            any_inconclusive |= !complete;
            output.line(&format!(
                "  instance={slot} epoch={} authenticator={} cursor={} active={} checkpoint={}",
                scalar(&snapshot["epoch"]),
                scalar(&snapshot["epoch_authenticator"]),
                scalar(&snapshot["cursor"]),
                scalar(&snapshot["active"]),
                if complete { "COMPLETE" } else { "INCOMPLETE" }
            ));
            let database_slot = if slot == crate::population::SHARED_SLOT as u64 {
                0
            } else {
                slot
            };
            if !seen_databases.insert(database_slot) {
                output.line(&format!("  instance={slot} shares database with instance={database_slot}; history compared once"));
                continue;
            }
            match read_history(&root, slot, database_slot, group) {
                Ok(history) => {
                    any_inconclusive |= history.capped;
                    output.line(&format!("  database={database_slot} rows={} history={} maybe_forked={} commit_log_forked={:?}",
                        history.records.len(), if history.capped { "CAPPED/INCONCLUSIVE" } else { "FULL" }, history.maybe_forked, history.commit_log_forked));
                    explain_flag(output, &history, selector.is_some());
                    histories.push(history);
                }
                Err(error) => {
                    any_inconclusive = true;
                    output.line(&format!(
                        "  database={database_slot} INCONCLUSIVE: {}",
                        safe_error(&format!("{error:#}"))
                    ));
                }
            }
        }
        let comparison = compare_history(&histories);
        mismatch_count += comparison.mismatch_count;
        if histories.len() > 1 && comparison.common_sequences == 0 {
            any_inconclusive = true;
        }
        output.line(&format!("  history shared_sequence_comparisons={} authenticator_mismatches={} excluded_anchor_or_removal_records={}", comparison.common_sequences, comparison.mismatch_count, comparison.exclusions));
        for (sequence, left, right) in comparison.mismatches.iter().take(FLAG_ERROR_ROWS) {
            output.line(&format!(
                "  HISTORY MISMATCH sequence={sequence} instances={left},{right}"
            ));
        }
        any_inconclusive |= !render_rollcall(output, &status, group);
        if output.full() {
            any_inconclusive = true;
            break;
        }
    }
    output.line(&format!(
        "Result: state/history mismatches={mismatch_count}; coverage={}",
        if any_inconclusive {
            "INCONCLUSIVE (see missing/capped/incomplete evidence)"
        } else {
            "all loaded histories complete at compared common sequences"
        }
    ));
    output.line("Equal checkpoints and common history do not prove that unobserved intermediate states never diverged.");
    Ok(())
}

fn validate_group(group: &str) -> Result<()> {
    ensure!(
        !group.is_empty()
            && group.len() <= 128
            && group.len().is_multiple_of(2)
            && group.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid group selector"
    );
    Ok(())
}

fn open_database(root: &Path, slot: u64, database_slot: u64) -> Result<SqliteConnection> {
    let config: Value = serde_json::from_slice(&read_bounded(
        &root.join(format!("instance-{slot}.json")),
        CONFIG_BYTES,
    )?)?;
    let key = config["database_key"]
        .as_str()
        .context("database key is missing")?;
    ensure!(key.len() == 64, "invalid database key length");
    let key = hex::decode(key).context("invalid database key encoding")?;
    ensure!(key.len() == 32, "invalid database key length");
    let path = root.join(format!("instance-{database_slot}.db3"));
    ensure!(regular_size(&path)?.is_some(), "database file is missing");
    let salt = read_bounded(
        &root.join(format!("instance-{database_slot}.db3.sqlcipher_salt")),
        64,
    )?;
    let salt = std::str::from_utf8(&salt)?.trim();
    ensure!(salt.len() == 32, "invalid database salt length");
    let salt = hex::decode(salt).context("invalid database salt encoding")?;
    let mut connection = SqliteConnection::establish(&readonly_uri(&path)?)?;
    connection.batch_execute(&format!("PRAGMA key=\"x'{}'\"; PRAGMA cipher_plaintext_header_size=32; PRAGMA cipher_salt=\"x'{}'\"; PRAGMA query_only=ON; PRAGMA busy_timeout=1000;", hex::encode(key), hex::encode(salt)))?;
    Ok(connection)
}

fn readonly_uri(path: &Path) -> Result<String> {
    let mut encoded = String::from("file:");
    for byte in path.to_str().context("database path is not UTF-8")?.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded.push_str("?mode=ro");
    Ok(encoded)
}

fn read_history(root: &Path, slot: u64, database_slot: u64, group: &str) -> Result<History> {
    use xmtp_db::schema::{groups::dsl as groups, local_commit_log::dsl as log};
    let mut connection = open_database(root, slot, database_slot)?;
    let id = hex::decode(group)?;
    let flags: Option<(bool, String, Option<bool>)> = groups::groups
        .filter(groups::id.eq(&id))
        .select((
            groups::maybe_forked,
            sql::<Text>("coalesce(substr(fork_details,1,2048),'')"),
            groups::is_commit_log_forked,
        ))
        .first(&mut connection)
        .optional()
        .context("read stored fork flags")?;
    let (maybe_forked, fork_details, commit_log_forked) =
        flags.context("saved group is missing from database")?;
    // Bound every variable field before it enters Rust. Oversized authenticators
    // make the result inconclusive, since their full values cannot be compared.
    let oversized: i64 = log::local_commit_log
        .filter(log::group_id.eq(&id))
        .filter(sql::<Bool>(
            "length(last_epoch_authenticator)>64 OR length(applied_epoch_authenticator)>64",
        ))
        .count()
        .get_result(&mut connection)?;
    ensure!(oversized == 0, "commit authenticator size cap reached");
    let mut records = log::local_commit_log
        .filter(log::group_id.eq(&id))
        .order(log::rowid.asc())
        .limit(HISTORY_ROWS + 1)
        .select((
            log::rowid,
            log::group_id,
            log::commit_sequence_id,
            log::last_epoch_authenticator,
            log::commit_result,
            log::applied_epoch_number,
            log::applied_epoch_authenticator,
            sql::<Nullable<Text>>("substr(error_message,1,2048)"),
            sql::<Nullable<Text>>("NULL"),
            sql::<Nullable<Binary>>("NULL"),
            sql::<Nullable<Text>>("substr(commit_type,1,64)"),
        ))
        .load::<LocalCommitLog>(&mut connection)
        .context("read local commit history")?;
    let capped = records.len() > HISTORY_ROWS as usize;
    records.truncate(HISTORY_ROWS as usize);
    Ok(History {
        slot,
        records,
        capped,
        maybe_forked,
        commit_log_forked,
        fork_details,
    })
}

fn comparable(record: &LocalCommitLog) -> bool {
    record.commit_sequence_id > 0
        && !record.last_epoch_authenticator.is_empty()
        && record.commit_type.as_deref() != Some("Welcome")
        && record.commit_type.as_deref() != Some("RemovedFromGroup")
        && !(record.commit_result == CommitResult::Success
            && record.applied_epoch_authenticator == record.last_epoch_authenticator)
}

fn compare_history(histories: &[History]) -> Comparison {
    let mut comparison = Comparison::default();
    let mut sequences: BTreeMap<i64, BTreeMap<u64, &[u8]>> = BTreeMap::new();
    for history in histories {
        for record in &history.records {
            if !comparable(record) {
                comparison.exclusions += 1;
                continue;
            }
            let previous = sequences.entry(record.commit_sequence_id).or_default();
            if previous.get(&history.slot).is_some_and(|authenticator| {
                *authenticator == record.applied_epoch_authenticator.as_slice()
            }) {
                continue;
            }
            for (slot, authenticator) in previous.iter() {
                comparison.common_sequences += 1;
                if *authenticator != record.applied_epoch_authenticator.as_slice() {
                    comparison.mismatch_count += 1;
                    if comparison.mismatches.len() < FLAG_ERROR_ROWS {
                        comparison.mismatches.push((
                            record.commit_sequence_id,
                            *slot,
                            history.slot,
                        ));
                    }
                }
            }
            previous.insert(history.slot, &record.applied_epoch_authenticator);
        }
    }
    comparison
}

fn explain_flag(output: &mut BoundedOutput, history: &History, show_records: bool) {
    if !history.maybe_forked && history.fork_details.is_empty() {
        return;
    }
    let sequence = history
        .fork_details
        .strip_prefix("Message epoch mismatch at sequence ")
        .and_then(|value| value.parse::<i64>().ok());
    output.line(&format!(
        "    flag cause={} sequence={}",
        if sequence.is_some() {
            "message epoch mismatch"
        } else {
            "unrecognized stored diagnostic"
        },
        sequence.map_or_else(|| "unknown".into(), |sequence| sequence.to_string())
    ));
    let Some(sequence) = sequence else {
        return;
    };
    let matching: Vec<_> = history
        .records
        .iter()
        .filter(|record| record.commit_sequence_id == sequence)
        .collect();
    if matching.is_empty() {
        output.line("    no matching commit record: the rejected envelope may be a message, or history is incomplete");
    }
    for record in matching.into_iter().take(FLAG_ERROR_ROWS) {
        output.line(&format!(
            "    rejected sequence={} result={:?} epoch={} error={}",
            record.commit_sequence_id,
            record.commit_result,
            record.applied_epoch_number,
            safe_error(record.error_message.as_deref().unwrap_or("none"))
        ));
        if let Some(previous) = history.records.iter().rev().find(|previous| {
            previous.rowid < record.rowid
                && previous.commit_result == CommitResult::Success
                && comparable(previous)
        }) {
            output.line(&format!(
                "    preceding success sequence={} epoch={} same_authenticator_as_rejection={}",
                previous.commit_sequence_id,
                previous.applied_epoch_number,
                previous.applied_epoch_authenticator == record.applied_epoch_authenticator
            ));
        }
        if show_records {
            output.line(&format!(
                "    row={} type={} last_authenticator={} applied_authenticator={}",
                record.rowid,
                record.commit_type.as_deref().unwrap_or("none"),
                hex::encode(
                    &record.last_epoch_authenticator[..record
                        .last_epoch_authenticator
                        .len()
                        .min(AUTHENTICATOR_BYTES)]
                ),
                hex::encode(
                    &record.applied_epoch_authenticator[..record
                        .applied_epoch_authenticator
                        .len()
                        .min(AUTHENTICATOR_BYTES)]
                )
            ));
        }
    }
}

fn compare_current(instances: &[(&Value, &Value)]) -> (bool, bool, usize) {
    let active: Vec<_> = instances
        .iter()
        .filter(|(_, group)| group["active"] == true)
        .map(|(_, group)| *group)
        .collect();
    let equal = active.windows(2).all(|pair| {
        ["epoch", "epoch_authenticator", "members", "metadata"]
            .iter()
            .all(|field| pair[0][*field] == pair[1][*field])
    });
    let cursor_equal = active
        .windows(2)
        .all(|pair| pair[0]["cursor"] == pair[1]["cursor"]);
    (equal, cursor_equal, active.len())
}

fn checkpoint_complete(checkpoint: &Value) -> bool {
    checkpoint.get("failure") == Some(&Value::Null)
        && checkpoint["topics"].as_array().is_some_and(|topics| {
            topics.iter().all(|topic| {
                topic["target"]
                    .as_u64()
                    .zip(topic["processed"].as_u64())
                    .is_some_and(|(target, processed)| processed >= target)
                    && topic["unresolved_welcomes"]
                        .as_array()
                        .is_some_and(Vec::is_empty)
            })
        })
}

fn render_rollcall(output: &mut BoundedOutput, status: &Value, group: &str) -> bool {
    let calls: Vec<_> = status["rollcall"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|call| call["group_id"] == group)
        .collect();
    let missing = |expected: &Value, received: &Value| -> usize {
        expected
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| {
                !received
                    .as_array()
                    .is_some_and(|received| received.contains(entry))
            })
            .count()
    };
    let sync_missing: usize = calls
        .iter()
        .map(|call| missing(&call["expected_installations"], &call["sync_received"]))
        .sum();
    let stream_missing: usize = calls
        .iter()
        .map(|call| {
            missing(
                &call["expected_stream_installations"],
                &call["stream_received"],
            )
        })
        .sum();
    output.line(&format!("  rollcall tokens={} missing_sync_deliveries={sync_missing} missing_stream_deliveries={stream_missing}", calls.len()));
    !calls.is_empty()
        && sync_missing == 0
        && stream_missing == 0
        && calls.iter().all(|call| {
            [
                "expected_installations",
                "expected_stream_installations",
                "sync_received",
                "stream_received",
            ]
            .iter()
            .all(|field| call[*field].is_array())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record(sequence: i64, auth: u8) -> LocalCommitLog {
        LocalCommitLog {
            rowid: sequence as i32,
            group_id: [1_u8; 16].into(),
            commit_sequence_id: sequence,
            last_epoch_authenticator: vec![0; 32],
            commit_result: CommitResult::Success,
            applied_epoch_number: sequence,
            applied_epoch_authenticator: vec![auth; 32],
            error_message: None,
            sender_inbox_id: None,
            sender_installation_id: None,
            commit_type: None,
        }
    }

    fn history(slot: u64, records: Vec<LocalCommitLog>) -> History {
        History {
            slot,
            records,
            capped: false,
            maybe_forked: false,
            commit_log_forked: Some(false),
            fork_details: String::new(),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn compares_common_history_and_excludes_removal_and_welcome() {
        let mut removal = record(3, 0);
        removal.commit_type = Some("RemovedFromGroup".into());
        let mut welcome = record(4, 9);
        welcome.last_epoch_authenticator.clear();
        let histories = [
            history(0, vec![record(1, 1), record(2, 2), removal, welcome]),
            history(
                1,
                vec![record(1, 1), record(2, 8), record(3, 3), record(4, 4)],
            ),
        ];
        let compared = compare_history(&histories);
        assert_eq!(compared.common_sequences, 2);
        assert_eq!(compared.mismatches, vec![(2, 0, 1)]);
        assert_eq!(compared.exclusions, 2);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn read_only_uri_refuses_writes_and_missing_files() {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("database ?# name.db3");
        let mut writer = SqliteConnection::establish(path.to_str().unwrap())?;
        writer.batch_execute("CREATE TABLE proof (value INTEGER);")?;
        drop(writer);
        let mut reader = SqliteConnection::establish(&readonly_uri(&path)?)?;
        reader.batch_execute("PRAGMA query_only=ON;")?;
        assert!(
            reader
                .batch_execute("INSERT INTO proof VALUES (1);")
                .is_err()
        );
        assert!(
            SqliteConnection::establish(&readonly_uri(&temp.path().join("missing.db3"))?).is_err()
        );
        assert!(!temp.path().join("missing.db3").exists());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn encrypted_history_keeps_empty_anchors_and_reports_the_row_cap() {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let key = hex::encode([7_u8; 32]);
        let salt = hex::encode([9_u8; 16]);
        let group = hex::encode([1_u8; 16]);
        std::fs::write(
            root.join("instance-0.json"),
            serde_json::to_vec(&json!({"database_key":key}))?,
        )?;
        std::fs::write(root.join("instance-0.db3.sqlcipher_salt"), &salt)?;
        let database = root.join("instance-0.db3");
        let mut writer = SqliteConnection::establish(database.to_str().unwrap())?;
        writer.batch_execute(&format!("PRAGMA key=\"x'{key}'\"; PRAGMA cipher_plaintext_header_size=32; PRAGMA cipher_salt=\"x'{salt}'\";
            CREATE TABLE groups (id BLOB PRIMARY KEY, maybe_forked BOOLEAN NOT NULL, fork_details TEXT NOT NULL, is_commit_log_forked BOOLEAN);
            CREATE TABLE local_commit_log (rowid INTEGER PRIMARY KEY, group_id BLOB NOT NULL, commit_sequence_id INTEGER NOT NULL, last_epoch_authenticator BLOB NOT NULL,
                commit_result INTEGER NOT NULL, applied_epoch_number INTEGER NOT NULL, applied_epoch_authenticator BLOB NOT NULL, error_message TEXT,
                sender_inbox_id TEXT, sender_installation_id BLOB, commit_type TEXT);
            INSERT INTO groups VALUES (x'{group}',0,'',NULL);
            INSERT INTO local_commit_log VALUES (1,x'{group}',0,x'',1,0,x'',NULL,NULL,NULL,'Welcome');"))?;
        let loaded = read_history(root, 0, 0, &group)?;
        assert_eq!(loaded.records.len(), 1);
        assert!(loaded.records[0].last_epoch_authenticator.is_empty());
        assert!(loaded.records[0].applied_epoch_authenticator.is_empty());
        assert!(!loaded.capped);
        writer.batch_execute(&format!("WITH RECURSIVE numbers(n) AS (SELECT 2 UNION ALL SELECT n+1 FROM numbers WHERE n<{})
            INSERT INTO local_commit_log SELECT n,x'{group}',n,x'00',1,n,x'01',NULL,NULL,NULL,'MetadataUpdate' FROM numbers;", HISTORY_ROWS + 1))?;
        drop(writer);
        let loaded = read_history(root, 0, 0, &group)?;
        assert_eq!(loaded.records.len(), HISTORY_ROWS as usize);
        assert!(loaded.capped);
        let mut reader = open_database(root, 0, 0)?;
        assert!(
            reader
                .batch_execute("DELETE FROM local_commit_log;")
                .is_err()
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn rejects_live_inspection_and_bounds_malformed_status() {
        let temp = tempfile::tempdir()?;
        std::fs::write(
            temp.path().join("status.json"),
            serde_json::to_vec(&json!({"phase":"chaos"}))?,
        )?;
        let output = inspect(temp.path(), None);
        assert!(output.contains("requires stopped writers"));
        assert!(output.lines().count() <= OUTPUT_LINES);
        let left = json!({"active":true,"epoch":4,"epoch_authenticator":"a","members":[1],"metadata":"private","cursor":8});
        let mut right = left.clone();
        assert_eq!(
            compare_current(&[(&Value::Null, &left), (&Value::Null, &right)]),
            (true, true, 2)
        );
        right["metadata"] = json!("changed");
        assert!(!compare_current(&[(&Value::Null, &left), (&Value::Null, &right)]).0);
    }
}
