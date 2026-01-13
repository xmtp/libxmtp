pub mod log;

use crate::{LogParser, Rule};
use anyhow::{Context, Result, bail};
use pest::{Parser, iterators::Pair};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock, RwLockWriteGuard},
};
use tracing::warn;
use xmtp_common::Event;

#[derive(Debug, PartialEq, Eq)]
pub enum Value {
    String(String),
    Bytes(Vec<u8>),
    Int(i64),
    Object(HashMap<String, Value>),
    Array(Vec<Self>),
    Boolean(bool),
    None,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_indented(f, 0)
    }
}

impl Value {
    fn fmt_indented(&self, f: &mut std::fmt::Formatter<'_>, indent: usize) -> std::fmt::Result {
        match self {
            Value::String(s) => write!(f, "{}", s),
            Value::Bytes(b) => write!(f, "{:?}", b),
            Value::Int(i) => write!(f, "{}", i),
            Value::Object(obj) => {
                if obj.is_empty() {
                    return write!(f, "{{}}");
                }
                let indent_str = "  ".repeat(indent);
                let inner_indent = "  ".repeat(indent + 1);
                writeln!(f, "{{")?;
                let mut first = true;
                for (k, v) in obj.iter() {
                    if !first {
                        writeln!(f, ",")?;
                    }
                    first = false;
                    write!(f, "{}{}: ", inner_indent, k)?;
                    v.fmt_indented(f, indent + 1)?;
                }
                writeln!(f)?;
                write!(f, "{}}}", indent_str)
            }
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Boolean(b) => write!(f, "{}", b),
            Value::None => write!(f, "null"),
        }
    }

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

    fn as_str(&self) -> Result<&str> {
        match self {
            Self::String(str) => Ok(&str),
            _ => bail!("{self:?} is not string"),
        }
    }

    fn as_int(&self) -> Result<i64> {
        match self {
            Self::Int(int) => Ok(*int),
            _ => bail!("{self:?} is not string"),
        }
    }

    fn as_obj(&self) -> Result<&HashMap<String, Self>> {
        match self {
            Self::Object(obj) => Ok(obj),
            _ => bail!("{self:?} is not string"),
        }
    }
}

#[derive(Debug)]
pub struct LogEvent {
    event: Event,
    inbox: String,
    context: HashMap<String, Value>,
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
                warn!("Missing key for pair: {pair_str}");
                continue;
            };
            let Some(value) = pair_inner.next().and_then(|p| Value::from(p).ok()) else {
                warn!("Unable to parse value for pair: {pair_str}");
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
        let inbox = &event.inbox;
        match event.event {
            Event::ClientCreated => {
                self.clients
                    .entry(inbox.to_string())
                    .or_insert_with(|| ClientState::new(None));
            }
            Event::CreatedDM => {
                let client = self.clients.get_mut(inbox).context("Missing client")?;
                let group_id = ctx("group_id")?.as_str()?;
                let mut dm = client
                    .groups
                    .entry(group_id.to_string())
                    .or_insert_with(|| GroupState::new())
                    .update();

                dm.dm_target = Some(ctx("target_inbox")?.as_str()?.to_string());
                dm.created_at = Some(ctx("timestamp")?.as_int()?);
            }
            _ => {}
        }

        Ok(())
    }
}

struct ClientState {
    name: Option<String>,
    groups: HashMap<String, Arc<RwLock<GroupState>>>,
}

impl ClientState {
    fn new(name: Option<String>) -> Self {
        Self {
            name,
            groups: HashMap::default(),
        }
    }
}

#[derive(Clone, Default)]
struct GroupState {
    prev: Option<Arc<RwLock<Self>>>,
    dm_target: Option<InboxId>,
    created_at: Option<i64>,
    epoch: Option<i64>,
    members: HashSet<InboxId>,
}

impl GroupState {
    fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self::default()))
    }
}

trait GroupStateExt {
    fn update(&mut self) -> RwLockWriteGuard<'_, GroupState>;
}
impl GroupStateExt for Arc<RwLock<GroupState>> {
    fn update(&mut self) -> RwLockWriteGuard<'_, GroupState> {
        let new_group = GroupState {
            prev: Some(self.clone()),
            ..self.read().unwrap().clone()
        };
        *self = Arc::new(RwLock::new(new_group));
        self.write().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use xmtp_common::Event;

    use crate::state::{LogEvent, Value};

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_log_parse() {
        let line = r#" INFO update_conversation_message_disappear_from_ns:sync_until_intent_resolved:sync_until_intent_resolved_inner:sync_with_conn:process_messages:process_message: xmtp_mls::identity_updates: ➣ Updating group membership. Calculating which installations need to be added / removed. {group_id: "c7ffe79e510aa970877222b3b4409d1c", old_membership: GroupMembership { members: {"0505be93287e66b32191a47e4107d0379fb34ed7b769f1b8437e733e178ed05b": 380, "f535576f351c902ce5319e46e74f949e835cc9c4bee6112e7b6a532320433e30": 379}, failed_installations: [] }, new_membership: GroupMembership { members: {"0505be93287e66b32191a47e4107d0379fb34ed7b769f1b8437e733e178ed05b": 380, "f535576f351c902ce5319e46e74f949e835cc9c4bee6112e7b6a532320433e30": 379}, failed_installations: [] }, inbox: "33e30", timestamp: 1767908059951353776}"#;

        let event = LogEvent::from(line)?;
        assert_eq!(event.event, Event::MembershipInstallationDiff);

        let group_id = event.context.get("group_id");
        assert!(group_id.is_some());
        assert_eq!(group_id?.as_str()?, "c7ffe79e510aa970877222b3b4409d1c");

        let new_membership = event.context.get("new_membership")?.as_obj()?;
        let members = new_membership.get("members")?.as_obj()?;
        assert_eq!(
            *members.get("0505be93287e66b32191a47e4107d0379fb34ed7b769f1b8437e733e178ed05b")?,
            Value::Int(380)
        );
        assert_eq!(
            *members.get("f535576f351c902ce5319e46e74f949e835cc9c4bee6112e7b6a532320433e30")?,
            Value::Int(379)
        );

        assert_eq!(
            event.context.get("timestamp")?.as_int()?,
            1767908059951353776
        );
    }
}
