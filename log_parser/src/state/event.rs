use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use pest::Parser;
use xmtp_common::Event;

use crate::{LogParser, Rule, state::Value};

#[derive(Debug)]
pub struct LogEvent {
    pub event: Event,
    pub inbox: String,
    pub context: HashMap<String, Value>,
}

impl LogEvent {
    pub fn event_name(&self) -> &str {
        self.event.metadata().doc
    }

    pub fn inbox(&self) -> &str {
        &self.inbox
    }

    pub fn timestamp_str(&self) -> String {
        self.context
            .get("timestamp")
            .and_then(|v| v.as_int().ok())
            .map(|ts| ts.to_string())
            .unwrap_or_default()
    }

    pub fn timestamp(&self) -> i64 {
        self.context
            .get("timestamp")
            .and_then(|v| v.as_int().ok())
            .unwrap_or(0)
    }

    pub fn context_entries(&self) -> Vec<(String, String)> {
        self.context
            .iter()
            .filter(|(k, _)| *k != "timestamp") // timestamp is handled separately
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect()
    }

    pub fn group_id(&self) -> Option<&str> {
        self.context.get("group_id").and_then(|v| v.as_str().ok())
    }

    pub fn from(line_str: &str) -> Result<Self> {
        let line = LogParser::parse(Rule::line, line_str)?;
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

        let inbox = context
            .remove("inbox")
            .with_context(|| format!("{line_str} is missing inbox field."))?
            .as_str()?
            .to_string();

        Ok(Self {
            event: event_meta.event,
            inbox,
            context,
        })
    }
}
