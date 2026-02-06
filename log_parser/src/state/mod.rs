pub mod assertions;
pub mod event;
pub mod ui;
pub mod value;

use crate::state::{
    assertions::{LogAssertion, epoch_continuity::EpochContinuityAssertion},
    event::TIME_KEY,
};
use anyhow::{Context, Result};
pub use event::LogEvent;
use parking_lot::{RwLock, RwLockWriteGuard};
use std::{
    collections::HashMap,
    iter::Peekable,
    sync::{Arc, Weak},
};
pub use value::Value;
use xmtp_common::Event;

type InstallationId = String;
type GroupId = String;
type EpochNumber = i64;

#[derive(Default)]
pub struct LogState {
    pub grouped_epochs: RwLock<HashMap<GroupId, HashMap<EpochNumber, Epoch>>>,
    pub clients: RwLock<HashMap<InstallationId, Arc<RwLock<ClientState>>>>,
}

#[derive(Default)]
struct Epoch {
    pub states: HashMap<InstallationId, Vec<Arc<RwLock<GroupState>>>>,
}

impl LogState {
    pub fn build<'a>(mut lines: Peekable<impl Iterator<Item = &'a str>>) -> Arc<Self> {
        let mut state = Self::default();

        while let Ok(event) =
            LogEvent::from(&mut lines).inspect_err(|e| tracing::error!("Parsing err: {e:?}"))
        {
            if let Err(err) = state.ingest(Arc::new(event)) {
                tracing::warn!("{err:?}");
            };
        }

        if let Err(err) = EpochContinuityAssertion::assert(&state) {
            tracing::error!("Continuity error: {err}");
        };

        Arc::new(state)
    }

    fn ingest(&mut self, event: Arc<LogEvent>) -> Result<()> {
        let ctx = |key: &str| -> Result<&Value> {
            event
                .context(key)
                .with_context(|| format!("Missing context field {key}"))
        };

        let installation = &event.installation;
        let mut clients = self.clients.write();
        let client = match event.event {
            Event::ClientCreated => clients
                .entry(installation.to_string())
                .or_insert_with(|| Arc::new(RwLock::new(ClientState::new(&installation, None)))),
            _ => clients.get_mut(installation).context("Missing client")?,
        };
        let mut client = client.write();
        let group_id = ctx("group_id").and_then(|id| id.as_str()).ok();

        match (group_id, event.event) {
            (_, Event::AssociateName) => {
                client.name = Some(ctx("name")?.as_str()?.to_string());
            }
            (Some(group_id), raw_event) => {
                {
                    let mut epochs = self.grouped_epochs.write();
                    if !epochs.contains_key(group_id) {
                        epochs.insert(group_id.to_string(), HashMap::new());
                    }
                }

                let mut group = client.update_group(group_id, &event);

                if let Ok(epoch) = ctx("epoch").and_then(|e| e.as_int()) {
                    group.epoch = Some(epoch);
                }

                match raw_event {
                    Event::CreatedDM => {
                        group.dm_target = Some(ctx("target_inbox")?.as_str()?.to_string());
                        group.created_at = Some(ctx(TIME_KEY)?.as_int()?);
                    }
                    Event::MLSGroupEpochUpdated => {
                        group.previous_epoch = Some(ctx("previous_epoch")?.as_int()?);
                        group.cursor = Some(ctx("cursor")?.as_int()?);
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        client.events.push(event);

        Ok(())
    }
}

pub struct ClientState {
    pub name: Option<String>,
    pub events: Vec<Arc<LogEvent>>,
    pub groups: HashMap<String, Arc<RwLock<GroupState>>>,
    pub inst: String,
}

impl ClientState {
    fn new(inst: &str, name: Option<String>) -> Self {
        Self {
            name,
            events: Vec::new(),
            groups: HashMap::default(),
            inst: inst.to_string(),
        }
    }

    fn update_group(
        &mut self,
        group_id: &str,
        event: &Arc<LogEvent>,
    ) -> RwLockWriteGuard<'_, GroupState> {
        if !self.groups.contains_key(group_id) {
            let state = GroupState::new(&self.inst, event);
            self.groups.insert(group_id.to_string(), state.clone());
            return self.groups[group_id].write();
        }

        self.groups.get_mut(group_id).unwrap().update(event)
    }
}

#[derive(Clone)]
pub struct GroupState {
    pub prev: Option<Arc<RwLock<Self>>>,
    pub next: Option<Weak<RwLock<Self>>>,
    pub installation_id: String,
    pub event: Arc<LogEvent>,
    pub dm_target: Option<InstallationId>,
    pub created_at: Option<i64>,
    pub previous_epoch: Option<i64>,
    pub epoch: Option<i64>,
    // Group states from other clients in the same epoch
    pub correlations: Vec<Arc<RwLock<Self>>>,
    pub cursor: Option<i64>,
    pub members: HashMap<InstallationId, Weak<RwLock<ClientState>>>,
    pub problems: Vec<GroupStateProblem>,
}

#[derive(Clone)]
pub struct GroupStateProblem {
    description: String,
    severity: Severity,
}

#[derive(Clone)]
pub enum Severity {}

impl IntoIterator for &GroupState {
    type IntoIter = GroupStateIterator;
    type Item = Arc<RwLock<GroupState>>;
    fn into_iter(self) -> Self::IntoIter {
        self.clone().into_iter()
    }
}
impl IntoIterator for GroupState {
    type IntoIter = GroupStateIterator;
    type Item = Arc<RwLock<GroupState>>;
    fn into_iter(self) -> Self::IntoIter {
        GroupStateIterator {
            current: None,
            staged: Some(Arc::new(RwLock::new(self))),
        }
    }
}

pub struct GroupStateIterator {
    current: Option<Arc<RwLock<GroupState>>>,
    staged: Option<Arc<RwLock<GroupState>>>,
}

impl Iterator for GroupStateIterator {
    type Item = Arc<RwLock<GroupState>>;
    fn next(&mut self) -> Option<Self::Item> {
        let current = self.staged.take()?;
        let staged = current.read().next.as_ref().and_then(Weak::upgrade);

        self.staged = staged;
        self.current = Some(current);
        self.current.clone()
    }
}

impl GroupState {
    fn new(inst: &str, event: &Arc<LogEvent>) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            installation_id: inst.to_string(),
            event: event.clone(),
            prev: None,
            next: None,
            dm_target: None,
            created_at: None,
            previous_epoch: None,
            epoch: None,
            correlations: Vec::new(),
            cursor: None,
            members: HashMap::new(),
            problems: Vec::new(),
        }))
    }
}

trait GroupStateExt {
    fn update(&mut self, event: &Arc<LogEvent>) -> RwLockWriteGuard<'_, GroupState>;
    fn beginning(&self) -> Result<Arc<RwLock<GroupState>>>;
    fn traverse(&self) -> GroupStateIterator;
}
impl GroupStateExt for Arc<RwLock<GroupState>> {
    fn update(&mut self, event: &Arc<LogEvent>) -> RwLockWriteGuard<'_, GroupState> {
        let prev = self.clone();
        let new_group = GroupState {
            prev: Some(prev.clone()),
            problems: vec![],
            event: event.clone(),
            ..self.read().clone()
        };
        *self = Arc::new(RwLock::new(new_group));
        // Double-link the chain with a weak.
        prev.write().next = Some(Arc::downgrade(self));

        self.write()
    }
    fn beginning(&self) -> Result<Self> {
        let mut r = self.clone();
        loop {
            let Some(prev) = r.read().prev.clone() else {
                break;
            };
            r = prev;
        }

        Ok(r.clone())
    }
    fn traverse(&self) -> GroupStateIterator {
        self.read().clone().into_iter()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, time::Duration};

    use crate::state::{LogEvent, LogState, Value};
    use tracing_subscriber::fmt;
    use xmtp_common::{Event, TestWriter};
    use xmtp_mls::tester;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_logging() {
        let writer = TestWriter::new();

        let subscriber = fmt::Subscriber::builder()
            .with_writer(writer.clone())
            .with_level(true)
            .with_ansi(false)
            .finish();

        let _guard = tracing::subscriber::set_default(subscriber);

        tester!(bo);
        tester!(alix);
        let (group, _) = bo.test_talk_in_new_group_with(&alix).await?;
        tester!(caro);
        group.add_members(&[caro.inbox_id()]).await?;
        group.update_group_name("Rocking".to_string()).await?;

        for client in &[&alix, &bo, &caro] {
            client.sync_all_welcomes_and_groups(None).await?;
        }

        xmtp_common::time::sleep(Duration::from_millis(200)).await;

        let log = writer.as_string();
        let lines = log.split("\n").peekable();

        let state = LogState::build(lines);

        let grouped_epochs = state.grouped_epochs.read();
        tracing::warn!("Groups: {}", grouped_epochs.len());
        for (key, epochs) in &*grouped_epochs {
            tracing::warn!("Group id: {key} Epochs: {}", epochs.len());
            // Assert that epoch transitions happen in chronological order
            let mut timestamps = HashSet::new();

            for (_epoch_num, epoch) in epochs.iter() {
                for (_inst, states) in &epoch.states {
                    for state in states {
                        let t = state.read().event.time;
                        assert!(!timestamps.contains(&t));
                        timestamps.insert(t);
                    }
                }
            }

            for (i, epoch) in epochs {
                tracing::warn!("Epoch {i} has {} group states", epoch.states.len());
            }
        }
    }

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
