//! Bounded inspection of structured evidence. Never emit raw database keys.
pub(crate) mod forks;

use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use anyhow::{Result, ensure};
use serde_json::Value;

use crate::ledger::{bounded_json, read_bounded, regular_size};

const OUTPUT_LINES: usize = 160;
const LINE_BYTES: usize = 512;
const INPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_DEPTH: usize = 12;
const MAX_VISITS: usize = 8192;
const LOG_TAIL_BYTES: u64 = 512 * 1024;
const LOG_RECORD_BYTES: usize = 64 * 1024;
const RECENT_OPERATION_ROWS: usize = 32;
const RECENT_ERROR_ROWS: usize = 24;
const ROUND_DIRECTORY_CAP: usize = 256;

pub(crate) struct BoundedOutput {
    output: String,
    lines: usize,
    max_lines: usize,
    line_bytes: usize,
    truncated: bool,
}

impl BoundedOutput {
    pub(crate) fn new(max_lines: usize, line_bytes: usize) -> Self {
        Self {
            output: String::new(),
            lines: 0,
            max_lines,
            line_bytes,
            truncated: false,
        }
    }

    pub(crate) fn line(&mut self, text: &str) {
        if self.lines >= self.max_lines.saturating_sub(1) {
            self.truncated = true;
            return;
        }
        let mut bytes = 0;
        for character in text.chars() {
            let character = if character.is_control() {
                ' '
            } else {
                character
            };
            if bytes + character.len_utf8() > self.line_bytes {
                self.truncated = true;
                break;
            }
            self.output.push(character);
            bytes += character.len_utf8();
        }
        self.output.push('\n');
        self.lines += 1;
    }

    fn full(&self) -> bool {
        self.lines >= self.max_lines.saturating_sub(1)
    }

    pub(crate) fn finish(mut self) -> String {
        if self.truncated {
            self.output.push_str("[output capped]\n");
        }
        self.output
    }
}

pub(crate) fn status(path: &Path) -> String {
    let mut output = BoundedOutput::new(OUTPUT_LINES, LINE_BYTES);
    match read_json(&path.join("status.json")) {
        Ok(value) => render_status(&mut output, &value),
        Err(error) => output.line(&format!("Cannot inspect status: {error}")),
    }
    output.finish()
}

pub(crate) fn inspect(path: &Path, group: Option<&str>, database: Option<&str>) -> String {
    let mut output = BoundedOutput::new(OUTPUT_LINES, LINE_BYTES);
    let result = (|| -> Result<()> {
        ensure!(
            !(group.is_some() && database.is_some()),
            "select a group or a database"
        );
        if let Some(name) = database {
            ensure!(name.len() <= LINE_BYTES, "database selector is too long");
            let records = read_json(&path.join("databases.json"))?;
            let records = records
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid database records"))?;
            let mut found = false;
            for record in records.iter().take(MAX_VISITS) {
                if record.get("name").and_then(Value::as_str) == Some(name) {
                    render_value(&mut output, "database", record, 0);
                    found = true;
                    break;
                }
            }
            ensure!(found, "database record not found");
        } else if let Some(group) = group {
            ensure!(group.len() <= LINE_BYTES, "group selector is too long");
            let checkpoint = if regular_size(&path.join("checkpoint.json"))?.is_some() {
                "checkpoint.json"
            } else {
                "status.json"
            };
            let mut found = false;
            if let Ok(value) = read_json(&path.join(checkpoint)) {
                render_findings(&mut output, &value["check"]["findings"], Some(group));
                let mut remaining = MAX_VISITS;
                let mut part = BoundedOutput::new(80, LINE_BYTES);
                for field in ["installations", "rollcall"] {
                    found |= find_group(&mut part, field, &value[field], group, 0, &mut remaining);
                }
                if !found {
                    found = find_group(&mut part, "checkpoint", &value, group, 0, &mut remaining);
                }
                append_output(&mut output, part);
            }
            found |= recent_records(&mut output, path, Some(group))?;
            ensure!(found, "group record not found within inspection limits");
        } else {
            if regular_size(&path.join("checkpoint.json"))?.is_some() {
                let value = read_json(&path.join("checkpoint.json"))?;
                for line in crate::evidence::summary_text(&value).lines() {
                    output.line(line);
                }
            } else if regular_size(&path.join("summary.md"))?.is_some() {
                let bytes = read_bounded(&path.join("summary.md"), INPUT_BYTES)?;
                let text = std::str::from_utf8(&bytes)?;
                for line in text.lines().take(OUTPUT_LINES + 1) {
                    output.line(line);
                }
            } else {
                match read_json(&path.join("status.json")) {
                    Ok(value) => render_status(&mut output, &value),
                    Err(error) => output.line(&format!("Cannot inspect status: {error}")),
                }
                recent_records(&mut output, path, None)?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        output.line(&format!("Cannot inspect bundle: {error}"));
    }
    output.finish()
}

fn append_output(output: &mut BoundedOutput, part: BoundedOutput) {
    for line in part.finish().lines() {
        output.line(line);
    }
}

fn render_status(output: &mut BoundedOutput, value: &Value) {
    let mut part = BoundedOutput::new(72, LINE_BYTES);
    for field in [
        "seed",
        "round",
        "phase",
        "verdict",
        "counters",
        "contention",
        "ops",
        "ok",
        "err",
        "faults",
        "bursts",
        "conflicts",
        "welcome_retries",
        "warnings",
        "stalls",
        "escalated_stalls",
        "recovery",
    ] {
        if let Some(value) = value.get(field) {
            if matches!(field, "counters" | "contention") {
                let mut field_output = BoundedOutput::new(8, LINE_BYTES);
                render_value(&mut field_output, field, value, 0);
                append_output(&mut part, field_output);
            } else if let Some(entries) = value.as_array() {
                part.line(&format!("{field}: {} entries", entries.len()));
            } else {
                part.line(&format!("{field}: {}", scalar(value)));
            }
        }
    }
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        part.line(&format!("error: {}", safe_error(error)));
    }
    render_faults(&mut part, value);
    for stream in value["streams"].as_array().into_iter().flatten().take(19) {
        let diagnostics = &stream["diagnostics"];
        part.line(&format!(
            "stream instance={} opens={} reopens={} eof={} errors={}",
            scalar(&stream["instance"]),
            scalar(&diagnostics["opens"]),
            scalar(&diagnostics["reopens"]),
            scalar(&diagnostics["eof_count"]),
            scalar(&diagnostics["error_count"])
        ));
    }
    render_findings(&mut part, &value["check"]["findings"], None);
    for instance in value["installations"]
        .as_array()
        .into_iter()
        .flatten()
        .take(24)
    {
        let groups = instance["groups"].as_array();
        let topics = instance["checkpoint"]["topics"].as_array();
        part.line(&format!(
            "instance={} stream_owner={} groups={} topics={} checkpoint_failure={}",
            scalar(&instance["instance"]),
            scalar(&instance["stream_owner"]),
            groups.map_or(0, Vec::len),
            topics.map_or(0, Vec::len),
            scalar(&instance["checkpoint"]["failure"])
        ));
        for group in groups.into_iter().flatten().take(4) {
            part.line(&format!(
                "  group={} epoch={} active={} membership={} fork_flags={}/{}",
                scalar(&group["group_id"]),
                scalar(&group["epoch"]),
                scalar(&group["active"]),
                scalar(&group["membership_state"]),
                scalar(&group["maybe_forked"]),
                scalar(&group["is_commit_log_forked"])
            ));
        }
        if part.full() {
            break;
        }
    }
    append_output(output, part);
}

pub(crate) fn render_faults(output: &mut BoundedOutput, value: &Value) {
    for fault in value["schedule"]["faults"]
        .as_array()
        .into_iter()
        .flatten()
        .take(8)
    {
        output.line(&format!(
            "fault={} instance={} start_ms={} duration_ms={}",
            scalar(&fault["kind"]),
            scalar(&fault["instance"]),
            scalar(&fault["start_ms"]),
            scalar(&fault["duration_ms"])
        ));
    }
}

fn scalar(value: &Value) -> String {
    if value.is_object() || value.is_array() {
        return "[structured]".into();
    }
    bounded_json(value, 128)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_else(|_| "[capped]".into())
}

fn newest_round(path: &Path) -> Result<Option<PathBuf>> {
    let rounds = path.join("rounds");
    let metadata = match std::fs::symlink_metadata(&rounds) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "invalid round directory"
    );
    let mut newest = None;
    for (index, entry) in std::fs::read_dir(rounds)?.enumerate() {
        ensure!(index < ROUND_DIRECTORY_CAP, "round directory cap reached");
        let entry = entry?;
        ensure!(entry.file_type()?.is_dir(), "invalid round entry");
        if let Ok(number) = entry.file_name().to_string_lossy().parse::<u64>()
            && newest.as_ref().is_none_or(|(current, _)| number > *current)
        {
            newest = Some((number, entry.path()));
        }
    }
    Ok(newest.map(|(_, path)| path))
}

fn read_jsonl_tail(path: &Path) -> Result<Vec<Value>> {
    let Some(length) = regular_size(path)? else {
        return Ok(Vec::new());
    };
    let start = length.saturating_sub(LOG_TAIL_BYTES);
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(LOG_TAIL_BYTES).read_to_end(&mut bytes)?;
    let mut lines = bytes.split(|byte| *byte == b'\n');
    if start > 0 {
        lines.next();
    }
    Ok(lines
        .take(MAX_VISITS)
        .filter(|line| !line.is_empty() && line.len() <= LOG_RECORD_BYTES)
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect())
}

fn recent_records(output: &mut BoundedOutput, path: &Path, group: Option<&str>) -> Result<bool> {
    let Some(round) = newest_round(path)? else {
        return Ok(false);
    };
    output.line(&format!(
        "Recent records: round {}",
        round.file_name().unwrap_or_default().to_string_lossy()
    ));
    for record in read_jsonl_tail(&round.join("faults.jsonl"))?.iter().take(8) {
        let fault = &record["fault"];
        output.line(&format!(
            "fault event={} kind={} instance={} start_ms={} duration_ms={}",
            scalar(&record["event"]),
            scalar(&fault["kind"]),
            scalar(&fault["instance"]),
            scalar(&fault["start_ms"]),
            scalar(&fault["duration_ms"])
        ));
    }
    let operations = read_jsonl_tail(&round.join("ops.jsonl"))?;
    let mut starts = BTreeMap::new();
    let mut rows = Vec::new();
    for record in &operations {
        let key = (record["burst"].as_u64(), record["instance"].as_u64());
        if record["event"] == "start" {
            starts.insert(key, &record["operation"]);
        }
        let operation = record
            .get("operation")
            .or_else(|| starts.get(&key).copied());
        if group.is_some_and(|group| operation.and_then(|op| op["group"].as_str()) != Some(group)) {
            continue;
        }
        let outcome = &record["outcome"];
        let result = if let Some(error) = outcome
            .get("Err")
            .and_then(Value::as_str)
            .or_else(|| outcome.get("error").and_then(Value::as_str))
        {
            safe_error(error)
        } else if record["event"] == "end" {
            "completed".into()
        } else {
            "started".into()
        };
        rows.push(format!(
            "operation instance={} burst={} kind={} group={} event={} duration_ms={} result={}",
            scalar(&record["instance"]),
            scalar(&record["burst"]),
            operation.map_or_else(|| "unknown".into(), |op| scalar(&op["kind"])),
            operation.map_or_else(|| "unknown".into(), |op| scalar(&op["group"])),
            scalar(&record["event"]),
            scalar(&record["duration_ms"]),
            result
        ));
    }
    let found = !rows.is_empty();
    for row in rows
        .iter()
        .skip(rows.len().saturating_sub(RECENT_OPERATION_ROWS))
    {
        output.line(row);
    }
    if group.is_none() {
        let logs = read_jsonl_tail(&round.join("logs.jsonl"))?;
        for record in logs.iter().rev().take(RECENT_ERROR_ROWS).rev() {
            if let Some(stderr) = record["stderr"].as_str() {
                output.line(&format!(
                    "child={} diagnostic={} truncated={}",
                    scalar(&record["slot"]),
                    diagnostic(stderr),
                    scalar(&record["truncated"])
                ));
            }
        }
    }
    Ok(found)
}

pub(crate) fn render_findings(output: &mut BoundedOutput, value: &Value, group: Option<&str>) {
    let mut findings: Vec<_> = value
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_VISITS)
        .filter(|finding| group.is_none_or(|group| finding["group_id"].as_str() == Some(group)))
        .collect();
    findings.sort_by_key(|finding| {
        let severity = match finding["verdict"].as_str() {
            Some("HARNESS") => 0,
            Some("FORK") => 1,
            Some("BRICK") => 2,
            Some("STALL") => 3,
            Some("WARN") => 4,
            _ => 5,
        };
        (severity, finding["escalated"] != true)
    });
    for finding in findings.iter().take(16) {
        output.line(&format!(
            "{} instance={} group={} topic={} repeats={} escalated={} detail={}",
            scalar(&finding["verdict"]),
            scalar(&finding["instance"]),
            scalar(&finding["group_id"]),
            scalar(&finding["topic"]),
            finding["repeated_rounds"].as_u64().unwrap_or_default(),
            finding["escalated"].as_bool().unwrap_or_default(),
            safe_error(finding["detail"].as_str().unwrap_or("missing detail"))
        ));
    }
    if findings.len() > 16 {
        output.line("[findings capped; violations shown first]");
    }
}

/// Keep error causes, but omit quoted values, key fields, and long encoded data.
fn safe_error(text: &str) -> String {
    let text = clipped(text, 1024);
    let lower = text.to_ascii_lowercase();
    if [
        "private_key",
        "wallet_key",
        "database_key",
        "encryption_key",
        "payload",
        "message content",
    ]
    .iter()
    .any(|field| lower.contains(field))
    {
        return format!("{} [sensitive error detail omitted]", diagnostic(text));
    }
    let mut output = String::new();
    let mut quote = None;
    let mut unquoted = String::new();
    for character in text.chars() {
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            }
        } else if character == '\"' || character == '\'' {
            quote = Some(character);
            unquoted.push_str("[value]");
        } else {
            unquoted.push(if character.is_control() {
                ' '
            } else {
                character
            });
        }
    }
    for word in unquoted.split_whitespace() {
        if !output.is_empty() {
            output.push(' ');
        }
        if word
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|part| part.len() >= 48)
        {
            output.push_str("[encoded value]");
        } else {
            output.push_str(word);
        }
        if output.len() >= 256 {
            break;
        }
    }
    clipped(&output, 256).to_string()
}

/// Free-form diagnostics can contain messages or keys. Emit only known categories.
fn diagnostic(text: &str) -> String {
    let text = clipped(text, LOG_RECORD_BYTES).to_ascii_lowercase();
    let mut categories = Vec::new();
    for (needle, label) in [
        ("database is locked", "database locked"),
        ("sqlite", "SQLite"),
        ("connection refused", "connection refused"),
        ("connection reset", "connection reset"),
        ("deadline", "deadline exceeded"),
        ("timeout", "timeout"),
        ("timed out", "timeout"),
        ("lease", "stream lease"),
        ("already active", "already active"),
        ("welcome", "Welcome processing"),
        ("group not found", "group not found"),
        ("not found", "not found"),
        ("panic", "panic"),
        ("storage", "storage"),
        ("retry", "retry"),
        ("permission", "permission"),
        ("unauthenticated", "unauthenticated"),
        ("invalid", "invalid data"),
        ("epoch", "MLS epoch"),
        ("fork", "fork flag"),
        ("closed", "closed"),
        ("shutdown", "shutdown"),
        ("failed", "failure"),
        ("error", "error"),
        ("warn", "warning"),
        ("broken pipe", "broken pipe"),
        ("busy", "busy"),
        ("disk full", "disk full"),
        ("unavailable", "unavailable"),
    ] {
        if text.contains(needle) && !categories.contains(&label) {
            categories.push(label);
        }
    }
    if categories.is_empty() {
        "unclassified diagnostic [text private]".into()
    } else {
        categories.join(", ")
    }
}

fn read_json(path: &Path) -> Result<Value> {
    let bytes = read_bounded(path, INPUT_BYTES)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub(crate) fn render_value(output: &mut BoundedOutput, prefix: &str, value: &Value, depth: usize) {
    if output.full() {
        output.truncated = true;
        return;
    }
    if depth >= MAX_DEPTH {
        output.line(&format!("{prefix}: [depth capped]"));
        return;
    }
    match value {
        Value::Object(object) if !object.is_empty() => {
            if let Some(count) = object
                .get("omitted_commit_count")
                .and_then(Value::as_u64)
                .filter(|count| *count > 0)
            {
                output.line(&format!(
                    "{}.omitted_commit_count: {count}",
                    clipped(prefix, LINE_BYTES)
                ));
            }
            for (key, value) in object {
                if key == "omitted_commit_count" {
                    continue;
                }
                let key = clipped(key, LINE_BYTES);
                let prefix = clipped(prefix, LINE_BYTES);
                if sensitive_key(key) {
                    output.line(&format!("{prefix}.{key}: [private]"));
                } else {
                    render_value(output, &format!("{prefix}.{key}"), value, depth + 1);
                }
                if output.full() {
                    output.truncated = true;
                    break;
                }
            }
        }
        Value::Array(array) if !array.is_empty() => {
            for (index, value) in array.iter().enumerate() {
                render_value(
                    output,
                    &format!("{}[{index}]", clipped(prefix, LINE_BYTES)),
                    value,
                    depth + 1,
                );
                if output.full() {
                    output.truncated = true;
                    break;
                }
            }
        }
        value => match bounded_json(value, LINE_BYTES) {
            Ok(bytes) => output.line(&format!("{prefix}: {}", String::from_utf8_lossy(&bytes))),
            Err(_) => {
                output.line(&format!("{prefix}: [value exceeds line cap]"));
            }
        },
    }
}

fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.ends_with("_key")
        || key.contains("secret")
        || matches!(
            key.as_str(),
            "key"
                | "keys"
                | "privatekey"
                | "encryptionkey"
                | "wallet"
                | "wallets"
                | "payload"
                | "body"
                | "content"
                | "token"
                | "metadata"
                | "message"
        )
}

fn clipped(value: &str, limit: usize) -> &str {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn find_group(
    output: &mut BoundedOutput,
    prefix: &str,
    value: &Value,
    group: &str,
    depth: usize,
    remaining: &mut usize,
) -> bool {
    if *remaining == 0 || depth >= MAX_DEPTH || output.full() {
        return false;
    }
    *remaining -= 1;
    match value {
        Value::Object(object) => {
            if ["group", "group_id"]
                .iter()
                .any(|key| object.get(*key).and_then(Value::as_str) == Some(group))
                || object
                    .get("topic")
                    .and_then(Value::as_str)
                    .is_some_and(|topic| {
                        topic == group || (group.len() >= 32 && topic.ends_with(group))
                    })
            {
                render_value(output, prefix, value, 0);
                return true;
            }
            let mut found = false;
            for (key, value) in object {
                if key == group {
                    render_value(
                        output,
                        &format!("{prefix}.{}", clipped(key, LINE_BYTES)),
                        value,
                        0,
                    );
                    found = true;
                } else {
                    found |= find_group(
                        output,
                        &format!(
                            "{}.{}",
                            clipped(prefix, LINE_BYTES),
                            clipped(key, LINE_BYTES)
                        ),
                        value,
                        group,
                        depth + 1,
                        remaining,
                    );
                }
                if *remaining == 0 || output.full() {
                    break;
                }
            }
            found
        }
        Value::Array(array) => {
            let mut found = false;
            for (index, value) in array.iter().enumerate() {
                found |= find_group(
                    output,
                    &format!("{}[{index}]", clipped(prefix, LINE_BYTES)),
                    value,
                    group,
                    depth + 1,
                    remaining,
                );
                if *remaining == 0 || output.full() {
                    break;
                }
            }
            found
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn assert_bounded(output: &str) {
        assert!(output.lines().count() <= OUTPUT_LINES);
        assert!(output.len() <= OUTPUT_LINES * (LINE_BYTES + 1));
        assert!(!output.contains('\u{1b}'));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn error_redaction_keeps_nested_types_but_hides_encoded_values() {
        let cause = "OpenMlsProcessMessageWithAppData(OpenMls(ValidationError(WrongEpoch)))";
        assert_eq!(safe_error(cause), cause);
        assert_eq!(
            safe_error(&format!("secret=\"{}\"", "ab".repeat(32))),
            "secret=[value]"
        );
        assert_eq!(
            safe_error(&format!("Authenticator({})", "ab".repeat(32))),
            "[encoded value]"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn live_inspection_shows_recent_errors_without_private_values() {
        let temp = tempfile::tempdir()?;
        std::fs::create_dir_all(temp.path().join("rounds/2"))?;
        std::fs::write(
            temp.path().join("status.json"),
            br#"{"seed":17,"round":2,"phase":"chaos"}"#,
        )?;
        let records = [
            json!({"event":"start","burst":0,"instance":4,"operation":{"kind":"send","group":"abc","token":"private-message"}}),
            json!({"event":"end","burst":0,"instance":4,"duration_ms":7,"outcome":{"error":"StorageError: database is locked; value=\"private-message\""}}),
        ];
        let bytes = records
            .iter()
            .map(|record| serde_json::to_string(record).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(temp.path().join("rounds/2/ops.jsonl"), bytes)?;
        std::fs::write(
            temp.path().join("rounds/2/logs.jsonl"),
            serde_json::to_vec(
                &json!({"slot":4,"stderr":"WARN private-message database is locked wallet_key=super-secret","truncated":false}),
            )?,
        )?;
        let output = inspect(temp.path(), None, None);
        assert_bounded(&output);
        assert!(output.contains("phase: \"chaos\""));
        assert!(output.contains("StorageError: database is locked"));
        assert!(output.contains("child=4"));
        assert!(!output.contains("private-message"));
        assert!(!output.contains("super-secret"));
        let group = inspect(temp.path(), Some("abc"), None);
        assert!(group.contains("StorageError: database is locked"));
        assert_bounded(&group);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn violations_precede_warnings_and_replace_stale_summary() {
        let temp = tempfile::tempdir()?;
        let mut findings = vec![json!({"verdict":"WARN","detail":"warning"}); 200];
        findings.push(json!({"verdict":"BRICK","instance":4,"group_id":"abc","detail":"stream token missing"}));
        let checkpoint =
            json!({"seed":7,"round":2,"verdict":"BRICK","check":{"findings":findings}});
        std::fs::write(
            temp.path().join("checkpoint.json"),
            serde_json::to_vec(&checkpoint)?,
        )?;
        std::fs::write(temp.path().join("summary.md"), "stale summary")?;
        let output = inspect(temp.path(), None, None);
        assert_bounded(&output);
        assert!(!output.contains("stale summary"));
        assert!(output.find("stream token missing") < output.find("warning"));
        let group = inspect(temp.path(), Some("abc"), None);
        assert!(group.contains("stream token missing"));
        assert_bounded(&group);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn escalated_stalls_precede_other_stalls_in_capped_findings() {
        let mut findings = vec![json!({"verdict":"STALL", "detail":"pending"}); 20];
        findings.push(json!({
            "verdict":"STALL", "detail":"blocked", "instance":4,
            "repeated_rounds":3, "escalated":true
        }));
        let mut output = BoundedOutput::new(OUTPUT_LINES, LINE_BYTES);
        render_findings(&mut output, &json!(findings), None);
        let output = output.finish();
        assert!(
            output
                .lines()
                .next()
                .unwrap()
                .contains("repeats=3 escalated=true")
        );
        assert!(output.contains("findings capped"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn malformed_and_huge_inputs_have_capped_output() {
        let temp = tempfile::tempdir()?;
        std::fs::write(temp.path().join("summary.md"), "\u{1b}[2J\n".repeat(10_000))?;
        assert_bounded(&inspect(temp.path(), None, None));
        std::fs::write(temp.path().join("status.json"), "{".repeat(INPUT_BYTES + 1))?;
        assert_bounded(&status(temp.path()));
        std::fs::write(temp.path().join("checkpoint.json"), b"invalid")?;
        assert_bounded(&inspect(temp.path(), Some("group"), None));
        std::fs::write(temp.path().join("databases.json"), b"invalid")?;
        assert_bounded(&inspect(temp.path(), None, Some(&"x\n".repeat(10_000))));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn group_and_database_views_are_bounded_and_hide_keys() {
        let temp = tempfile::tempdir()?;
        let groups = json!({"groups": (0..1000).map(|_| json!({"group_id": "abc", "epoch": 7})).collect::<Vec<_>>()});
        std::fs::write(
            temp.path().join("checkpoint.json"),
            serde_json::to_vec(&groups)?,
        )?;
        let output = inspect(temp.path(), Some("abc"), None);
        assert_bounded(&output);
        assert!(output.contains("epoch: 7"));
        std::fs::write(
            temp.path().join("databases.json"),
            br#"[{"name":"one","key":"secret-key","files":[{"file":"one.db3","bytes":12}]}]"#,
        )?;
        let output = inspect(temp.path(), None, Some("one"));
        assert_bounded(&output);
        assert!(output.contains("[private]"));
        assert!(!output.contains("secret-key"));
    }
}
