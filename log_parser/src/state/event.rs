use crate::{LogParser, Rule, UIContextEntry, state::Value, ui::file_open::color_from_string};
use anyhow::{Context, Result, bail};
use parking_lot::Mutex;
use pest::Parser;
use slint::{Color, SharedString};
use std::{
    collections::HashMap,
    iter::Peekable,
    sync::Arc,
    sync::atomic::{AtomicI64, Ordering},
};
use xmtp_common::Event;

#[derive(Debug)]
pub struct LogEvent {
    pub event: Event,
    pub msg: &'static str,
    pub icon: &'static str,
    pub installation: String,
    pub context: HashMap<String, Value>,
    pub intermediate: String,
    pub line_number: usize,
    pub time: AtomicI64,
    pub problems: Mutex<Vec<String>>,
}

impl Ord for LogEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time().cmp(&other.time())
    }
}
impl Eq for LogEvent {}

impl PartialEq for LogEvent {
    fn eq(&self, other: &Self) -> bool {
        self.time() == other.time()
    }
}

impl PartialOrd for LogEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.time().partial_cmp(&other.time())
    }
}

pub(crate) const TIME_KEY: &str = "time";

impl LogEvent {
    pub fn parse<'a>(mut lines: Peekable<impl Iterator<Item = &'a str>>) -> Vec<Arc<LogEvent>> {
        let mut line_count = 0;
        let mut events = vec![];
        while let Ok(event) = Self::from(&mut lines, &mut line_count) {
            events.push(Arc::new(event));
        }

        events
    }

    pub fn added_inboxes(&self) -> Result<Vec<&str>> {
        let added_inboxes = self
            .context("added_inboxes")
            .context("Missing added_inboxes")?
            .as_array()?;
        let added = added_inboxes
            .iter()
            .map(|i| {
                i.as_obj()?
                    .get("inbox_id")
                    .context("Missing inbox_id")?
                    .as_str()
            })
            .collect::<Result<Vec<&str>>>()?;

        Ok(added)
    }

    pub fn context(&self, key: &str) -> Option<&Value> {
        self.context.get(key)
    }

    pub fn ui_context_entries(&self) -> Vec<UIContextEntry> {
        self.context
            .iter()
            .map(|(k, v)| UIContextEntry {
                key: SharedString::from(k),
                value: SharedString::from(v.to_string()),
            })
            .collect()
    }

    pub fn ui_group_color(&self) -> Option<Color> {
        let group_id = self.context("group_id")?.as_str().ok()?;
        Some(color_from_string(group_id))
    }

    pub fn time(&self) -> i64 {
        self.time.load(Ordering::Relaxed)
    }

    pub fn from<'a>(
        lines: &mut Peekable<impl Iterator<Item = &'a str>>,
        line_count: &mut usize,
    ) -> Result<Self> {
        let (line, line_str, line_number) = loop {
            let line_str = lines.next().context("End of file")?;
            *line_count += 1;
            if let Ok(line) = LogParser::parse(Rule::line, line_str) {
                break (line, line_str, *line_count);
            }
        };

        // There should only ever be one event per line.
        let Some(line) = line.last() else {
            bail!("Line has no events");
        };
        let mut line_inner = line.into_inner();
        let Some(event) = line_inner.find(|e| matches!(e.as_rule(), Rule::event)) else {
            bail!("Line has no events");
        };
        let Some(object) = line_inner.find(|p| matches!(p.as_rule(), Rule::object)) else {
            bail!("Line is missing object");
        };
        let event_str = event.as_str().trim();

        let Some(event_meta) = Event::METADATA.iter().find(|m| m.doc == event_str) else {
            bail!("Unable to find matching event for {event_str}");
        };

        let mut context = HashMap::new();
        for pair in object.into_inner() {
            if !matches!(pair.as_rule(), Rule::pair) {
                continue;
            }
            let pair_str = pair.as_str();
            let mut pair_inner = pair.into_inner();
            let Some(key) = pair_inner.next() else {
                tracing::warn!("Missing key for pair: {pair_str}");
                continue;
            };
            let Some(value) = pair_inner.next().and_then(|p| Value::from(p).ok()) else {
                tracing::warn!("Unable to parse value for pair: {pair_str}");
                continue;
            };

            context.insert(key.as_str().to_string(), value);
        }

        let installation = context
            .remove("inst")
            .with_context(|| format!("{line_str} is missing inst field."))?;
        let installation = installation.as_str()?.to_string();
        let time = context
            .remove(TIME_KEY)
            .with_context(|| format!("{line_str} is missing time field."))?;

        // Collect up the intermediate lines that don't parse,
        // stripping ANSI escape codes so they display cleanly in the UI.
        let mut intermediate = String::new();
        while lines.peek().is_some_and(|l| !l.contains("➣")) {
            if let Some(line) = lines.next() {
                *line_count += 1;
                intermediate.push_str(&strip_ansi_escapes(line));
                intermediate.push('\n');
            }
        }

        Ok(Self {
            event: event_meta.event,
            icon: event_meta.icon,
            msg: event_meta.doc,
            installation,
            context,
            intermediate,
            line_number,
            time: AtomicI64::new(time.as_int()?),
            problems: Mutex::default(),
        })
    }
}

/// Strip ANSI escape sequences (CSI sequences like color codes) from a string
/// so that raw log output displays cleanly in the UI.
fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.next() {
                // CSI sequence: ESC [ <params> <final byte>
                // Parameter bytes are in 0x30-0x3F, intermediate bytes in 0x20-0x2F,
                // and the final byte is in 0x40-0x7E.
                Some('[') => {
                    for c in chars.by_ref() {
                        let b = c as u32;
                        if (0x40..=0x7E).contains(&b) {
                            break;
                        }
                    }
                }
                // OSC sequence: ESC ] ... (terminated by BEL or ST)
                Some(']') => {
                    let mut prev = '\0';
                    for c in chars.by_ref() {
                        if c == '\x07' || (prev == '\x1b' && c == '\\') {
                            break;
                        }
                        prev = c;
                    }
                }
                // For other escape sequences (e.g. ESC followed by a single char),
                // just skip the ESC and that character.
                Some(_) => {}
                // Trailing ESC at end of string
                None => {}
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tracing_subscriber::fmt;
    use xmtp_common::TestWriter;
    use xmtp_mls::{client::ClientError, tester};

    use super::*;

    fn parse_one(line: &str) -> LogEvent {
        let mut iter = std::iter::once(line).peekable();
        let mut line_count = 0;
        LogEvent::from(&mut iter, &mut line_count).expect("expected parser to parse line")
    }

    fn parse_with_intermediate(first_line: &str, intermediate_line: &str) -> LogEvent {
        let mut iter = [first_line, intermediate_line, "no arrow terminator line"]
            .into_iter()
            .peekable();
        let mut line_count = 0;
        LogEvent::from(&mut iter, &mut line_count).expect("expected parser to parse line")
    }

    async fn capture_events<F, Fut>(f: F) -> Result<Vec<Arc<LogEvent>>, ClientError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<(), ClientError>>,
    {
        capture_events_with_error(f).await
    }

    async fn capture_events_with_error<F, Fut, E>(f: F) -> Result<Vec<Arc<LogEvent>>, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<(), E>>,
    {
        let writer = TestWriter::new();
        let subscriber = fmt::Subscriber::builder()
            .with_writer(writer.clone())
            .with_level(true)
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        f().await?;

        tokio::time::sleep(Duration::from_millis(500)).await;

        let log = writer.as_string();
        let lines = log.split('\n').peekable();
        let events = LogEvent::parse(lines);

        Ok(events)
    }

    trait EventLogs {
        // Get the installation id from the logs for the given name.
        // Only works if a name is associated with an AssociateName event.
        fn inst(&self, name: &str) -> Option<&String>;
        fn inbox(&self, name: &str) -> Option<&str>;
        fn events_for(&self, name: &str, event: Event) -> Vec<&Arc<LogEvent>>;
        // Returns an error if there is more or less than one event.
        fn one_event(&self, name: &str, event: Event) -> Result<&Arc<LogEvent>>;
    }
    impl EventLogs for Vec<Arc<LogEvent>> {
        fn inst(&self, name: &str) -> Option<&String> {
            self.iter()
                .filter(|e| matches!(e.event, Event::AssociateName))
                .find_map(|e| (e.context("name")?.as_str().ok()? == name).then(|| &e.installation))
        }

        fn inbox(&self, name: &str) -> Option<&str> {
            let inst = self.inst(name)?;
            self.iter()
                .find(|e| matches!(e.event, Event::ClientCreated) && e.installation == *inst)
                .and_then(|e| e.context("inbox_id").and_then(|v| v.as_str().ok()))
        }

        fn events_for(&self, name: &str, event: Event) -> Vec<&Arc<LogEvent>> {
            let inst = self.inst(name).expect("Installation not found");
            self.iter()
                .filter(|e| e.event == event && e.installation == *inst)
                .collect()
        }

        fn one_event(&self, name: &str, event: Event) -> Result<&Arc<LogEvent>> {
            let mut events = self.events_for(name, event);
            let Some(event) = events.pop() else {
                bail!("No events");
            };
            if !events.is_empty() {
                bail!("More than one event. ({} events)", events.len() + 1);
            }

            Ok(event)
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_log_and_parse_dm_creation() {
        let events = capture_events(|| async {
            tester!(alix, disable_workers);
            tester!(bo, disable_workers);

            alix.test_talk_in_dm_with(&bo).await?;

            Ok(())
        })
        .await?;

        events.one_event("alix", Event::ClientCreated)?;
        events.one_event("alix", Event::ClientDropped)?;
        events.one_event("bo", Event::ClientCreated)?;
        events.one_event("bo", Event::ClientDropped)?;
        events.one_event("alix", Event::CreatedDM)?;

        let welcome = events.one_event("bo", Event::ReceivedWelcome)?;
        assert_eq!(welcome.context("conversation_type")?.as_str()?, "dm");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_log_group_updates() {
        let events = capture_events(|| async {
            tester!(alix, disable_workers);
            tester!(bo, disable_workers);
            tester!(caro, disable_workers);

            let (alix_group, _) = alix.test_talk_in_new_group_with(&bo).await?;
            alix_group.add_members(&[caro.inbox_id()]).await?;

            bo.sync_all_welcomes_and_groups(None).await?;

            Ok(())
        })
        .await?;

        // Check for the log that says we added caro's inbox to the group.
        let event = events.one_event("bo", Event::MLSProcessedStagedCommit)?;
        let caro_inbox = events.inbox("caro")?;
        let added_inboxes = event.added_inboxes()?;
        assert!(added_inboxes.contains(&caro_inbox));
    }

    #[test]
    fn parses_processed_staged_commit_with_nested_arrays_objects_and_metadata_changes() {
        let line = r#"2026-02-26T21:27:03.407066Z  INFO update_group_name{group_name="Fellows" who=474169123a5f60e3bd91df11630a5c246c2ce2030e28ddc9394b83f08bc50b99}: xmtp_mls::groups::mls_sync: ➣ Processed staged commit. {group_id: "fe5e86db", actor_installation_id: "900c6a51", epoch: 3, epoch_auth: "f28cbd89", added_inboxes: [], removed_inboxes: [], left_inboxes: [], metadata_changes: [{"field_name":"group_name","old_value":"","new_value":"Fellows"}], cursor: 81032, originator: 0, time: 1772141223407060497, inst: "900c6a51"}"#;
        let event = parse_one(line);

        assert_eq!(event.msg, "Processed staged commit.");
        assert_eq!(event.installation, "900c6a51");
        assert_eq!(
            event.context("group_id").unwrap().as_str().unwrap(),
            "fe5e86db"
        );
        assert_eq!(event.context("epoch").unwrap().as_int().unwrap(), 3);
        assert_eq!(event.context("cursor").unwrap().as_int().unwrap(), 81032);

        let metadata_changes = event
            .context("metadata_changes")
            .expect("metadata_changes should be present")
            .as_obj()
            .expect("metadata_changes should parse as object")
            .get("field_name")
            .expect("field_name key should be present")
            .as_str()
            .expect("field_name should be a string");
        assert_eq!(metadata_changes, "group_name");

        let old_value = event
            .context("metadata_changes")
            .expect("metadata_changes should be present")
            .as_obj()
            .expect("metadata_changes should parse as object")
            .get("old_value")
            .expect("old_value key should be present")
            .as_str()
            .expect("old_value should be a string");
        assert_eq!(old_value, "");

        let new_value = event
            .context("metadata_changes")
            .expect("metadata_changes should be present")
            .as_obj()
            .expect("metadata_changes should parse as object")
            .get("new_value")
            .expect("new_value key should be present")
            .as_str()
            .expect("new_value should be a string");
        assert_eq!(new_value, "Fellows");

        let added_inboxes = event
            .context("added_inboxes")
            .expect("added_inboxes should be present");
        assert!(matches!(added_inboxes, Value::Array(values) if values.is_empty()));
    }

    #[test]
    fn parses_group_sync_complete_multiline_summary_with_intermediate_and_strips_ansi() {
        let first_line = r#"2026-02-26T21:27:03.344000Z  INFO xmtp_mls::groups::mls_sync: ➣ Group sync complete. {group_id: "abc12345", summary: Some(synced 2 messages, 0 failed 2 succeeded from cursor Some(Cursor { sequence_id: 12345, originator_id: 7 }), success: true, time: 1772141223999999999, inst: "deadbeef"}"#;
        let intermediate_line = "\u{1b}[31merror details from transcript\u{1b}[0m";

        let event = parse_with_intermediate(first_line, intermediate_line);

        assert_eq!(event.msg, "Group sync complete.");
        assert_eq!(event.installation, "deadbeef");

        let summary = event.context("summary").unwrap().as_str().unwrap();
        assert!(summary.contains("synced 2 messages"));
        assert!(summary.contains("sequence_id: 12345"));

        assert_eq!(event.intermediate.trim(), "error details from transcript");
    }

    #[test]
    fn parses_option_value_none_and_some_cursor_in_summary_like_payload() {
        let line = r#"2026-02-26T21:27:03.350000Z  INFO xmtp_mls::groups::mls_sync: ➣ Group sync complete. {group_id: "def67890", summary: Some(synced 0 messages, 0 failed 0 succeeded from cursor None), success: true, time: 1772141223000000000, inst: "cafebabe"}"#;
        let event = parse_one(line);

        assert_eq!(event.msg, "Group sync complete.");
        let summary = event.context("summary").unwrap().as_str().unwrap();
        assert!(summary.contains("cursor None"));
    }
}
