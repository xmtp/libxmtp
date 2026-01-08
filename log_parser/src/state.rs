use anyhow::{Context, Result, bail};
use pest::{Parser, iterators::Pair};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};
use tracing::warn;
use xmtp_common::Event;

use crate::{LogParser, Rule};

#[derive(Debug, PartialEq, Eq)]
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
            Rule::quoted_string => Self::String(pair_str.replace("\"", "").to_string()),
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
                        object.insert(key.as_str().replace("\"", "").to_string(), value);
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

    fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(str) => Some(&str),
            _ => None,
        }
    }

    fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(int) => Some(*int),
            _ => None,
        }
    }

    fn as_obj(&self) -> Option<&HashMap<String, Self>> {
        match self {
            Self::Object(obj) => Some(obj),
            _ => None,
        }
    }
}

#[derive(Debug)]
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
            let pair_str = pair.as_str();
            let mut pair_inner = pair.into_inner();
            let Some(key) = pair_inner.next() else {
                warn!("Missing key for pair: {pair_str}");
                continue;
            };
            let Some(value) = pair_inner.next().and_then(|p| Value::from(p).ok()) else {
                warn!("Unable to parse value for pair: {pair_str}");
                continue;
            };

            context.insert(key.as_str().to_string(), value);
        }

        Ok(Self {
            event: event_meta.event,
            context,
        })
    }
}

type InboxId = String;

struct State {
    // key: inbox_id
    clients: HashMap<InboxId, ClientState>,
}

impl State {
    fn ingest(&mut self, event: LogEvent) -> Result<()> {
        let ctx = |key: &str| {
            event
                .context
                .get(key)
                .with_context(|| format!("Missing context field {key}"))
        };
        match event.event {
            Event::ClientCreated => {
                let inbox_id = ctx("inbox_id")?.as_str().expect("InboxId should be str");
                self.clients
                    .entry(inbox_id.to_string())
                    .or_insert_with(|| ClientState::new(None));
            }
            _ => {}
        }

        Ok(())
    }
}

struct ClientState {
    name: Option<String>,
    groups: HashMap<Vec<u8>, Rc<RefCell<GroupState>>>,
}

impl ClientState {
    fn new(name: Option<String>) -> Self {
        Self {
            name,
            groups: HashMap::default(),
        }
    }
}

#[derive(Clone)]
struct GroupState {
    prev: Option<Rc<RefCell<Self>>>,
    dm_target: Option<InboxId>,
    created_at: Option<i64>,
    epoch: i64,
    members: HashSet<InboxId>,
}

impl GroupState {
    fn step(mut group: Rc<RefCell<Self>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            prev: Some(group.clone()),
            ..group.borrow().clone()
        }))
    }
}

#[cfg(test)]
mod tests {
    use xmtp_common::Event;

    use crate::state::{LogEvent, Value};

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_log_parse() {
        let line = r#"INFO; update_conversation_message_disappear_from_ns:sync_until_intent_resolved:sync_until_intent_resolved_inner:sync_with_conn:process_messages:process_message: xmtp_mls::identity_updates: ➣ Updating group membership. Calculating which installations need to be added / removed. {group_id: "f143ed03a0069366acb7aaad9c529542", old_membership: GroupMembership { members: {"b3fbe1fdc94398def04fa116f6c85bed3463a43e9e69d376156062edfae043d8": 365, "0976e879cb8b05477ec4673be925e9becf8fb1c2fff56488326a6ba0a06b4f0f": 366}, failed_installations: [] }, new_membership: GroupMembership { members: {"b3fbe1fdc94398def04fa116f6c85bed3463a43e9e69d376156062edfae043d8": 365, "0976e879cb8b05477ec4673be925e9becf8fb1c2fff56488326a6ba0a06b4f0f": 366}, failed_installations: [] }, timestamp: 1767884672122893682}"#;

        let event = LogEvent::from(line)?;
        assert_eq!(event.event, Event::MembershipInstallationDiff);

        let group_id = event.context.get("group_id");
        assert!(group_id.is_some());
        assert_eq!(group_id?.as_str()?, "f143ed03a0069366acb7aaad9c529542");

        let new_membership = event.context.get("new_membership")?.as_obj()?;
        let members = new_membership.get("members")?.as_obj()?;
        assert_eq!(
            *members.get("b3fbe1fdc94398def04fa116f6c85bed3463a43e9e69d376156062edfae043d8")?,
            Value::Int(365)
        );
        assert_eq!(
            *members.get("0976e879cb8b05477ec4673be925e9becf8fb1c2fff56488326a6ba0a06b4f0f")?,
            Value::Int(366)
        );

        assert_eq!(
            event.context.get("timestamp")?.as_int()?,
            1767884672122893682
        );
    }
}
