use crate::{LogParser, Rule, UIContextEntry, state::Value, ui::file_open::color_from_string};
use anyhow::{Context, Result, bail};
use pest::Parser;
use slint::{Color, SharedString};
use std::{collections::HashMap, iter::Peekable};
use xmtp_common::Event;

#[derive(Debug)]
pub struct LogEvent {
    pub event: Event,
    pub msg: &'static str,
    pub icon: &'static str,
    pub installation: String,
    pub context: HashMap<String, Value>,
    pub intermediate: String,
    pub time: i64,
}

pub(crate) const TIME_KEY: &str = "time_ms";

impl LogEvent {
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

    pub fn event_name(&self) -> &str {
        self.event.metadata().doc
    }

    pub fn installation(&self) -> &str {
        &self.installation
    }

    pub fn group_id(&self) -> Option<&str> {
        self.context.get("group_id").and_then(|v| v.as_str().ok())
    }

    pub fn from<'a>(lines: &mut Peekable<impl Iterator<Item = &'a str>>) -> Result<Self> {
        let (line, line_str) = loop {
            let line_str = lines.next().context("End of file")?;
            if let Ok(line) = LogParser::parse(Rule::line, line_str) {
                break (line, line_str);
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
            .with_context(|| format!("{line_str} is missing time_ms field."))?;

        // Collect up the intermediate lines that don't parse.
        let mut intermediate = String::new();
        while lines.peek().is_some_and(|l| !l.contains("➣")) {
            if let Some(line) = lines.next() {
                intermediate.push_str(line);
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
            time: time.as_int()?,
        })
    }
}
