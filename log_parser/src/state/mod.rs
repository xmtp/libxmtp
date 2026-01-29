pub mod event;
pub mod value;

use anyhow::{Context, Result};
pub use event::LogEvent;
use std::{
    collections::{HashMap, HashSet},
    iter::Peekable,
    sync::{Arc, RwLock, RwLockWriteGuard},
};
pub use value::Value;
use xmtp_common::Event;

type InstallationId = String;

pub struct LogState {
    pub clients: HashMap<InstallationId, ClientState>,
}

impl LogState {
    pub fn build<'a>(mut lines: Peekable<impl Iterator<Item = &'a str>>) -> Self {
        let mut state = Self {
            clients: HashMap::new(),
        };
        while let Ok(event) = LogEvent::from(&mut lines) {
            if let Err(err) = state.ingest(event) {
                tracing::warn!("{err:?}");
            };
        }

        state
    }

    fn ingest(&mut self, event: LogEvent) -> Result<()> {
        let ctx = |key: &str| -> Result<&Value> {
            event
                .context
                .iter()
                .find(|(k, _)| k == key)
                .with_context(|| format!("Missing context field {key}"))
                .map(|(_, v)| v)
        };

        let installation = &event.installation;
        let client = match event.event {
            Event::ClientCreated => self
                .clients
                .entry(installation.to_string())
                .or_insert_with(|| ClientState::new(None)),
            _ => self
                .clients
                .get_mut(installation)
                .context("Missing client")?,
        };

        match event.event {
            Event::AssociateName => {
                client.name = Some(ctx("name")?.as_str()?.to_string());
            }
            Event::CreatedDM => {
                let group_id = ctx("group_id")?.as_str()?;
                let mut dm = client
                    .groups
                    .entry(group_id.to_string())
                    .or_insert_with(|| GroupState::new())
                    .update();

                dm.dm_target = Some(ctx("target_inbox")?.as_str()?.to_string());
                dm.created_at = Some(ctx("time")?.as_int()?);
            }
            _ => {}
        }

        client.events.push(event);

        Ok(())
    }
}

pub struct ClientState {
    pub name: Option<String>,
    pub events: Vec<LogEvent>,
    pub groups: HashMap<String, Arc<RwLock<GroupState>>>,
}

impl ClientState {
    fn new(name: Option<String>) -> Self {
        Self {
            name,
            events: Vec::new(),
            groups: HashMap::default(),
        }
    }
}

#[derive(Clone, Default)]
pub struct GroupState {
    prev: Option<Arc<RwLock<Self>>>,
    dm_target: Option<InstallationId>,
    created_at: Option<i64>,
    epoch: Option<i64>,
    members: HashSet<InstallationId>,
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
    use crate::state::{LogEvent, Value};
    use xmtp_common::Event;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_log_parse() {
        let line = r#" INFO update_conversation_message_disappear_from_ns:sync_until_intent_resolved:sync_until_intent_resolved_inner:sync_with_conn:process_messages:process_message: xmtp_mls::identity_updates: ➣ Updating group membership. Calculating which installations need to be added / removed. {group_id: "c7ffe79e510aa970877222b3b4409d1c", old_membership: GroupMembership { members: {"0505be93287e66b32191a47e4107d0379fb34ed7b769f1b8437e733e178ed05b": 380, "f535576f351c902ce5319e46e74f949e835cc9c4bee6112e7b6a532320433e30": 379}, failed_installations: [] }, new_membership: GroupMembership { members: {"0505be93287e66b32191a47e4107d0379fb34ed7b769f1b8437e733e178ed05b": 380, "f535576f351c902ce5319e46e74f949e835cc9c4bee6112e7b6a532320433e30": 379}, failed_installations: [] }, inbox: "33e30", timestamp: 1767908059951353776}"#;
        let mut line = line.split('\n').peekable();

        let event = LogEvent::from(&mut line)?;
        assert_eq!(event.event, Event::MembershipInstallationDiff);

        let group_id = event.context("group_id");
        assert!(group_id.is_some());
        assert_eq!(group_id?.as_str()?, "c7ffe79e510aa970877222b3b4409d1c");

        let new_membership = event.context("new_membership")?.as_obj()?;
        let members = new_membership.get("members")?.as_obj()?;
        assert_eq!(
            *members.get("0505be93287e66b32191a47e4107d0379fb34ed7b769f1b8437e733e178ed05b")?,
            Value::Int(380)
        );
        assert_eq!(
            *members.get("f535576f351c902ce5319e46e74f949e835cc9c4bee6112e7b6a532320433e30")?,
            Value::Int(379)
        );

        assert_eq!(event.context("timestamp")?.as_int()?, 1767908059951353776);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_dm_created_with_unquoted_group_id() {
        let line = r#"2026-01-13T16:01:32.795843Z  INFO xmtp_mls::client: ➣ DM created. {group_id: 6dbafe8fc16699dfe3b59d60944150b3, target_inbox: "ab23790529e1fa52ed453e69d0d342f02bc8db8e2317f6229672dd0ca4f6d527", inbox: "2857d", timestamp: 1768320092795839419} group_id=6dbafe8fc16699dfe3b59d60944150b3 target_inbox="ab23790529e1fa52ed453e69d0d342f02bc8db8e2317f6229672dd0ca4f6d527" inbox=2857d"#;

        let event = LogEvent::from(&mut line.split('\n').peekable())?;
        assert_eq!(event.event, Event::CreatedDM);

        let group_id = event.context("group_id");
        assert!(group_id.is_some());
        // Unquoted hex string should be parsed as a string
        assert_eq!(group_id?.as_str()?, "6dbafe8fc16699dfe3b59d60944150b3");

        let target_inbox = event.context("target_inbox");
        assert!(target_inbox.is_some());
        assert_eq!(
            target_inbox?.as_str()?,
            "ab23790529e1fa52ed453e69d0d342f02bc8db8e2317f6229672dd0ca4f6d527"
        );

        assert_eq!(event.context("timestamp")?.as_int()?, 1768320092795839419);
    }
}
