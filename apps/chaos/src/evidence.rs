//! Freeze evidence only after the supervisor has observed every writer exit.

use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Result, ensure};
use serde_json::{Value, json};

use crate::ledger::{RunLedger, private_dir, private_file, regular_size, valid_name};

const SUMMARY_LINES: usize = 190;
const SUMMARY_LINE_BYTES: usize = 512;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const SUMMARY_GROUP_ROWS: usize = 48;
const SUMMARY_TOPIC_ROWS: usize = 48;
const SUMMARY_OPERATION_ROWS: usize = 24;

/// The supervisor creates this only from exit observations for all writers.
pub(crate) struct StoppedWriters {
    _private: (),
}

impl StoppedWriters {
    pub(crate) fn confirm(exited: &[bool]) -> Result<Self> {
        ensure!(!exited.is_empty(), "no writer exit observations");
        ensure!(
            exited.iter().all(|exited| *exited),
            "database writers are still running"
        );
        Ok(Self { _private: () })
    }
}

pub(crate) struct DatabaseCopy {
    pub(crate) source: PathBuf,
    pub(crate) name: String,
    pub(crate) key: Value,
}

/// The run directory is the bundle. Its existing ledgers retain the last rounds.
pub(crate) fn write_bundle(
    ledger: &RunLedger,
    _stopped: &StoppedWriters,
    summary: &Value,
    databases: &[DatabaseCopy],
) -> Result<PathBuf> {
    ledger.enforce_bounds()?;
    ledger.write_json("checkpoint.json", summary)?;
    ledger.write_bytes("summary.md", summary_text(summary).as_bytes())?;
    let directory = ledger.root().join("databases");
    private_dir(&directory)?;
    let mut records = Vec::new();
    for database in databases {
        valid_name(&database.name)?;
        let source = database.source.canonicalize()?;
        ensure!(
            source.starts_with(ledger.root()),
            "database is outside the run directory"
        );
        // inventory rejects links before canonicalization can hide one.
        ledger.enforce_bounds()?;
        let destination = directory.join(format!("{}.db3", database.name));
        let mut files = Vec::new();
        for suffix in ["", "-wal", "-shm", "-journal", ".sqlcipher_salt"] {
            let source = with_suffix(&source, suffix);
            if regular_size(&source)?.is_none() {
                ensure!(!suffix.is_empty(), "database is missing");
                continue;
            }
            let destination = with_suffix(&destination, suffix);
            let bytes = copy_stopped_file(ledger, &source, &destination)?;
            files.push(json!({
                "file": destination.strip_prefix(ledger.root())?.to_string_lossy(),
                "bytes": bytes,
            }));
        }
        records.push(json!({ "name": database.name, "key": database.key, "files": files }));
    }
    ledger.write_json("databases.json", &json!(records))?;
    ledger.enforce_bounds()?;
    Ok(ledger.root().to_path_buf())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn copy_stopped_file(ledger: &RunLedger, source: &Path, destination: &Path) -> Result<u64> {
    let length = regular_size(source)?.ok_or_else(|| anyhow::anyhow!("database is missing"))?;
    ledger.reserve(length, 1)?;
    let mut input = File::open(source)?;
    let mut output = private_file(destination, true)?;
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    let mut copied = 0_u64;
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        copied = copied.saturating_add(count as u64);
        ensure!(copied <= length, "database changed after writer shutdown");
        output.write_all(&buffer[..count])?;
    }
    ensure!(copied == length, "database changed after writer shutdown");
    output.sync_all()?;
    Ok(copied)
}

pub(crate) fn summary_text(summary: &Value) -> String {
    let mut output = crate::inspect::BoundedOutput::new(SUMMARY_LINES, SUMMARY_LINE_BYTES);
    output.line("# Chaos violation");
    output.line("Private evidence. Database keys and message content are sensitive.");
    output.line(&format!(
        "Seed: {}. Round: {}. Verdict: {}.",
        cell(&summary["seed"], 24),
        cell(&summary["round"], 24),
        cell(&summary["verdict"], 80),
    ));
    output.line("");
    output.line("## Findings");
    crate::inspect::render_findings(&mut output, &summary["check"]["findings"], None);
    output.line("");
    output.line("## Installation states");
    output.line(
        "| Instance / installation | Group | Epoch | Authenticator | Members | State | Omitted commits |",
    );
    output.line("| --- | --- | --- | --- | --- | --- | --- |");
    let mut group_rows = 0;
    for instance in summary["installations"].as_array().into_iter().flatten() {
        for group in instance["groups"].as_array().into_iter().flatten() {
            if group_rows >= SUMMARY_GROUP_ROWS {
                break;
            }
            output.line(&format!(
                "| {} / {} | {} | {} | {} | {} | {} | {} |",
                cell(&instance["instance"], 12),
                cell(&instance["installation_id"], 64),
                cell(&group["group_id"], 64),
                cell(&group["epoch"], 24),
                cell(&group["epoch_authenticator"], 64),
                group["members"].as_array().map_or(0, Vec::len),
                cell(&group["membership_state"], 24),
                group["omitted_commit_count"].as_u64().unwrap_or(0),
            ));
            group_rows += 1;
        }
        if group_rows >= SUMMARY_GROUP_ROWS {
            output.line("[installation rows capped; use inspect group]");
            break;
        }
    }
    output.line("");
    output.line("## Barrier obligations");
    output.line(
        "| Instance | Topic | Target H | Received F | Processed P | Unresolved welcomes | Cause |",
    );
    output.line("| --- | --- | --- | --- | --- | --- | --- |");
    let mut topic_rows = 0;
    for instance in summary["installations"].as_array().into_iter().flatten() {
        for topic in instance["checkpoint"]["topics"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if topic_rows >= SUMMARY_TOPIC_ROWS {
                break;
            }
            output.line(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |",
                cell(&instance["instance"], 12),
                cell(&topic["topic"], 128),
                cell(&topic["target"], 24),
                cell(&topic["received"], 24),
                cell(&topic["processed"], 24),
                cell(&topic["unresolved_welcomes"], 64),
                cell(&topic["cause"], 96),
            ));
            topic_rows += 1;
        }
        if topic_rows >= SUMMARY_TOPIC_ROWS {
            output.line("[topic rows capped; use inspect group]");
            break;
        }
    }
    output.line("");
    output.line("## Scheduled operations");
    crate::inspect::render_faults(&mut output, summary);
    let mut operation_rows = 0;
    for burst in summary["schedule"]["bursts"]
        .as_array()
        .into_iter()
        .flatten()
    {
        for operation in burst["operations"].as_array().into_iter().flatten() {
            if operation_rows >= SUMMARY_OPERATION_ROWS {
                break;
            }
            output.line(&format!(
                "{}: instance={} kind={} group={}",
                cell(&burst["recipe"], 64),
                cell(&operation["instance"], 12),
                cell(&operation["operation"]["kind"], 64),
                cell(&operation["operation"]["group"], 64),
            ));
            operation_rows += 1;
        }
        if operation_rows >= SUMMARY_OPERATION_ROWS {
            output.line("[operation rows capped]");
            break;
        }
    }
    output.line("");
    output.line("Use xchaos inspect --group <group> for group records.");
    output.line("Use xchaos inspect --db <name> for database copy records.");
    output.finish()
}

fn cell(value: &Value, cap: usize) -> String {
    let mut output = String::new();
    let serialized;
    let text = if let Some(text) = value.as_str() {
        text
    } else if let Ok(bytes) = crate::ledger::bounded_json(value, cap) {
        serialized = String::from_utf8_lossy(&bytes).into_owned();
        &serialized
    } else {
        "[value capped]"
    };
    for character in text.chars() {
        let character = if character.is_control() || character == '|' {
            ' '
        } else {
            character
        };
        if output.len() + character.len_utf8() > cap {
            break;
        }
        output.push(character);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::Limits;

    #[xmtp_common::test(unwrap_try = true)]
    async fn copy_requires_all_writers_to_exit_and_keeps_sidecars() {
        let temp = tempfile::tempdir()?;
        let root = temp.path().canonicalize()?;
        let ledger = RunLedger::new(&root, Limits::default())?;
        std::fs::write(root.join("live.db3"), b"database")?;
        std::fs::write(root.join("live.db3-wal"), b"wal")?;
        std::fs::write(
            root.join("live.db3.sqlcipher_salt"),
            b"00112233445566778899aabbccddeeff",
        )?;
        assert!(StoppedWriters::confirm(&[true, false]).is_err());
        assert!(StoppedWriters::confirm(&[]).is_err());
        assert!(!root.join("databases").exists());
        let stopped = StoppedWriters::confirm(&[true, true])?;
        write_bundle(
            &ledger,
            &stopped,
            &json!({"seed": 17, "verdict": "FORK"}),
            &[DatabaseCopy {
                source: root.join("live.db3"),
                name: "installation-1".into(),
                key: json!("secret"),
            }],
        )?;
        assert_eq!(
            std::fs::read(root.join("databases/installation-1.db3-wal"))?,
            b"wal"
        );
        assert!(root.join("databases.json").exists());
        assert_eq!(
            std::fs::read(root.join("databases/installation-1.db3.sqlcipher_salt"))?,
            b"00112233445566778899aabbccddeeff"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn summary_is_bounded_for_large_and_multiline_values() {
        let summary = json!({
            "seed": 3,
            "installations": (0..1000).map(|i| json!({
                "instance": i,
                "installation_id": "abc",
                "groups": [{"group_id": "def", "epoch": 17, "metadata": "x\n".repeat(1000), "omitted_commit_count": 512}],
                "checkpoint": {"topics": [{"topic": "03def", "target": 19, "processed": 18, "cause": "ProcessingPending"}]}
            })).collect::<Vec<_>>(),
            "schedule": {"bursts": [{"recipe": "join", "operations": [{"instance": 0, "operation": {"kind": "add", "group": "def"}}]}]},
        });
        let output = summary_text(&summary);
        assert!(output.lines().count() < 200);
        assert!(output.len() <= SUMMARY_LINES * (SUMMARY_LINE_BYTES + 1));
        assert!(output.contains("Seed: 3"));
        assert!(output.contains("| 0 / abc | def | 17 |"));
        assert!(output.contains("ProcessingPending"));
        assert!(output.contains("kind=add group=def"));
        assert!(output.contains("Omitted commits"));
        assert!(output.contains("| 512 |"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn healthy_budget_keeps_room_for_stopped_database_copies() {
        const HARD_BYTES: u64 = 64 * 1024;
        const DATABASE_BYTES: usize = 25 * 1024;
        const OTHER_BYTES: usize = 5 * 1024;
        let temp = tempfile::tempdir()?;
        let root = temp.path().canonicalize()?;
        let ledger = RunLedger::new(
            &root,
            Limits {
                bytes: HARD_BYTES,
                files: 32,
            },
        )?;
        std::fs::write(root.join("live.db3"), vec![0_u8; DATABASE_BYTES])?;
        ledger.enforce_healthy_bounds()?;
        std::fs::write(root.join("recent.jsonl"), vec![0_u8; OTHER_BYTES])?;
        assert!(ledger.enforce_healthy_bounds().is_err());
        ledger.enforce_bounds()?;
        write_bundle(
            &ledger,
            &StoppedWriters::confirm(&[true])?,
            &json!({"seed": 1, "verdict": "BRICK"}),
            &[DatabaseCopy {
                source: root.join("live.db3"),
                name: "one".into(),
                key: json!("private"),
            }],
        )?;
        assert_eq!(
            std::fs::metadata(root.join("databases/one.db3"))?.len(),
            DATABASE_BYTES as u64
        );
        ledger.enforce_bounds()?;
    }
}
