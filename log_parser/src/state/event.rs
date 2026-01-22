use crate::{LogParser, Rule, state::Value};
use anyhow::{Context, Result, bail};
use pest::Parser;
use std::iter::Peekable;
use xmtp_common::Event;

#[derive(Debug)]
pub struct LogEvent {
    pub event: Event,
    pub installation: String,
    pub context: Vec<(String, Value)>,
    pub intermediate: String,
}

impl LogEvent {
    pub fn event_name(&self) -> &str {
        self.event.metadata().doc
    }

    pub fn inbox(&self) -> &str {
        &self.installation
    }

    pub fn timestamp_str(&self) -> String {
        self.timestamp().to_string()
    }

    pub fn timestamp(&self) -> i64 {
        self.context
            .iter()
            .find(|(k, _)| k == "time")
            .and_then(|(_, v)| v.as_int().ok())
            .unwrap_or(0)
    }

    pub fn context_entries(&self) -> Vec<(String, String)> {
        self.context
            .iter()
            .filter(|(k, _)| *k != "time") // timestamp is handled separately
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect()
    }

    pub fn group_id(&self) -> Option<&str> {
        self.context
            .iter()
            .find(|(k, _)| k == "group_id")
            .and_then(|(_, v)| v.as_str().ok())
    }

    pub fn from<'a>(lines: &mut Peekable<impl Iterator<Item = &'a str>>) -> Result<Self> {
        let line_str = lines.peek().context("End of file")?;
        let line = LogParser::parse(Rule::line, line_str)?;

        let line_str = lines.next().expect("We peeked and found a line.");

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

        let mut context = Vec::new();
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

            context.push((key.as_str().to_string(), value));
        }

        let (_, inbox) = context
            .extract_if(.., |(k, _)| k == "inst")
            .collect::<Vec<_>>()
            .pop()
            .with_context(|| format!("{line_str} is missing inst field."))?;
        let inbox = inbox.as_str()?.to_string();

        // Collect up the intermediate lines that don't parse.
        let mut intermediate = String::new();
        while Self::from(lines).is_err() {
            let Some(line) = lines.next() else {
                break;
            };
            intermediate.push_str(line);
            intermediate.push('\n');
        }

        Ok(Self {
            event: event_meta.event,
            installation: inbox,
            context,
            intermediate,
        })
    }
}
