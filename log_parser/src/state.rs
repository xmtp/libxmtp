use anyhow::{Result, bail};
use pest::Parser;
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use xmtp_common::Event;

use crate::{LogParser, Rule};

enum Value {
    String(String),
    Bytes(Vec<u8>),
    Int(i64),
}

struct LogEvent {
    event: Event,
    context: HashMap<String, Value>,
}

impl LogEvent {
    fn from(line: &str) -> Result<Self> {
        let line = LogParser::parse(Rule::line, line)?;
        // There should only ever be one event per line.
        let Some(line) = line.last() else {
            bail!("Line has no events");
        };
        let mut line_inner = line.into_inner();
        let Some(event) = line_inner.find(|e| matches!(e.as_rule(), Rule::event)) else {
            bail!("Line has no events");
        };
        let event_str = event.as_str().trim();

        let Some(event_meta) = Event::METADATA.iter().find(|m| m.doc == event_str) else {
            bail!("Unable to find matching event for {event_str}");
        };

        let mut context = HashMap::new();

        Ok(Self {
            event: event_meta.event,
            context,
        })
    }
}

struct State {
    // key: inbox_id
    clients: HashMap<String, ClientState>,
}

struct ClientState {
    name: Option<String>,
    groups: HashMap<Vec<u8>, Rc<RefCell<GroupState>>>,
}

struct GroupState {
    prev: Option<Rc<RefCell<Self>>>,
    created_at: Option<i64>,
}

impl GroupState {
    fn step(mut group: Rc<RefCell<Self>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            prev: Some(group.clone()),
            ..*group.borrow()
        }))
    }
}
