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

    pub fn removed_inboxes(&self) -> Result<Vec<&str>> {
        let added_inboxes = self
            .context("removed_inboxes")
            .context("Missing removed_inboxes")?
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
        fn events(&self, name: &str, event: Event) -> Vec<&Arc<LogEvent>>;
        fn n_events(&self, name: &str, event: Event, count: usize) -> Result<Vec<&Arc<LogEvent>>>;
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

        fn events(&self, name: &str, event: Event) -> Vec<&Arc<LogEvent>> {
            let inst = self.inst(name).expect("Installation not found");
            self.iter()
                .filter(|e| e.event == event && e.installation == *inst)
                .collect()
        }

        fn one_event(&self, name: &str, event: Event) -> Result<&Arc<LogEvent>> {
            let mut events = self.events(name, event);
            let Some(event) = events.pop() else {
                bail!("No events");
            };
            if !events.is_empty() {
                bail!("More than one event. ({} events)", events.len() + 1);
            }

            Ok(event)
        }

        fn n_events(&self, name: &str, event: Event, count: usize) -> Result<Vec<&Arc<LogEvent>>> {
            let events = self.events(name, event);
            if events.len() != count {
                bail!(
                    "Expected {count} events, got {} for {name} and {event}",
                    events.len()
                );
            }

            Ok(events)
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
            caro.sync_all_welcomes_and_groups(None).await?;

            alix_group.remove_members(&[caro.inbox_id()]).await?;

            bo.sync_all_welcomes_and_groups(None).await?;
            caro.sync_all_welcomes_and_groups(None).await?;

            Ok(())
        })
        .await?;

        // Check for the log that says we added caro's inbox to the group.
        let bo_events = events.n_events("bo", Event::MLSProcessedStagedCommit, 2)?;

        let caro_inbox = events.inbox("caro")?;

        // Caro was added
        let added_inboxes = bo_events[0].added_inboxes()?;
        assert!(added_inboxes.contains(&caro_inbox));

        // Caro was removed
        let removed_inboxes = bo_events[1].removed_inboxes()?;
        assert!(removed_inboxes.contains(&caro_inbox));

        // Caro created a group from welcome
        let caro_event = events.one_event("caro", Event::ReceivedWelcome)?;
        assert_eq!(caro_event.context("conversation_type")?.as_str()?, "group");
    }
}
