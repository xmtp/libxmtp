use anyhow::{Result, bail};
use pest::{Parser, iterators::Pair};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use xmtp_common::Event;

use crate::{LogParser, Rule};

enum Value {
    String(String),
    Bytes(Vec<u8>),
    Int(i64),
    Object(HashMap<String, Value>),
    Array(Vec<Self>),
    Boolean(bool),
    None,
}

impl Value {
    fn from(pair: Pair<'_, Rule>) -> Result<Self> {
        let pair_str = pair.as_str();
        let val = match pair.as_rule() {
            Rule::quoted_string => Self::String(pair_str.to_string()),
            Rule::number => Self::Int(pair_str.parse()?),
            Rule::array => {
                let mut array = Vec::new();
                for item in pair.into_inner() {
                    if let Ok(item) = Value::from(item) {
                        array.push(item);
                    }
                }
                Self::Array(array)
            }
            Rule::object => {
                let mut object = HashMap::new();
                for pair in pair.into_inner() {
                    let mut pair_inner = pair.into_inner();
                    let Some(key) = pair_inner.next() else {
                        continue;
                    };
                    let Some(value) = pair_inner.next() else {
                        continue;
                    };

                    // Do this so we don't completely omit the line if a single value fails to parse.
                    if let Ok(value) = Self::from(value) {
                        object.insert(key.as_str().to_string(), value);
                    }
                }
                Self::Object(object)
            }
            Rule::boolean => match pair_str {
                "true" => Self::Boolean(true),
                "false" => Self::Boolean(false),
                _ => unreachable!(),
            },
            Rule::null => Self::None,
            _ => bail!("Unsupportd rule encountered while parsing context."),
        };

        Ok(val)
    }
}

pub struct LogEvent {
    event: Event,
    context: HashMap<String, Value>,
}

impl LogEvent {
    pub fn from(line: &str) -> Result<Self> {
        let line = LogParser::parse(Rule::line, line)?;
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
            let mut pair_inner = pair.into_inner();

            let Some(key) = pair_inner.find(|p| matches!(p.as_rule(), Rule::key)) else {
                continue;
            };
            let Some(value) = pair_inner.find(|p| {
                matches!(
                    p.as_rule(),
                    Rule::object
                        | Rule::array
                        | Rule::quoted_string
                        | Rule::number
                        | Rule::boolean
                        | Rule::null
                )
            }) else {
                continue;
            };
        }

        for field in event_meta.context_fields {}

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
