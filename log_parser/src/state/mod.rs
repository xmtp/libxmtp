pub mod assertions;
pub mod event;
pub mod ui;
pub mod value;

use crate::state::assertions::{LogAssertion, epoch_continuity::EpochContinuityAssertion};
use anyhow::{Context, Result};
pub use event::LogEvent;
use parking_lot::{Mutex, MutexGuard, RwLock, RwLockWriteGuard};
use std::{
    collections::{BTreeMap, HashMap},
    iter::Peekable,
    sync::{Arc, Weak},
};
pub use value::Value;
use xmtp_common::Event;

type InstallationId = String;
type GroupId = String;
type EpochNumber = i64;
type EpochAuth = String;

#[derive(Default)]
pub struct LogState {
    pub grouped_epochs: RwLock<HashMap<GroupId, HashMap<InstallationId, Epochs>>>,
    pub clients: RwLock<HashMap<InstallationId, Arc<RwLock<ClientState>>>>,
}

#[derive(Default)]
pub struct Epochs {
    pub outer_events: Arc<RwLock<Vec<Arc<LogEvent>>>>,
    pub epochs: BTreeMap<EpochNumber, Epoch>,
}

#[derive(Default)]
pub struct Epoch {
    pub auth: Option<EpochAuth>,
    pub states: Vec<Arc<Mutex<GroupState>>>,
}

impl LogState {
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    pub fn ingest_all<'a>(&self, mut lines: Peekable<impl Iterator<Item = &'a str>>) {
        while let Ok(event) =
            LogEvent::from(&mut lines).inspect_err(|e| tracing::error!("Parsing err: {e:?}"))
        {
            if let Err(err) = self.ingest(Arc::new(event)) {
                tracing::warn!("{err:?}");
            };
        }

        if let Err(err) = EpochContinuityAssertion::assert(&self) {
            tracing::error!("Continuity error: {err}");
        };

        // sort everything by time
        self.clients.write().values().for_each(|c| c.write().sort());
    }

    fn ingest(&self, event: Arc<LogEvent>) -> Result<()> {
        let ctx = |key: &str| -> Result<&Value> {
            event
                .context(key)
                .with_context(|| format!("Missing context field {key}"))
        };

        let installation = &event.installation;
        let mut clients = self.clients.write();
        let mut client = match event.event {
            Event::ClientCreated => {
                let inbox_id = ctx("inbox_id")?.as_str()?;
                clients.entry(installation.to_string()).or_insert_with(|| {
                    Arc::new(RwLock::new(ClientState::new(&installation, inbox_id, None)))
                })
            }
            _ => clients.get_mut(installation).context("Missing client")?,
        }
        .write();
        let group_id = ctx("group_id").and_then(|id| id.as_str()).ok();

        match (group_id, event.event) {
            (_, Event::AssociateName) => {
                client.name = Some(ctx("name")?.as_str()?.to_string());
            }
            (_, Event::ClientCreated) => {
                client.num_clients += 1;

                if client.num_clients > 1 {
                    event.problems.lock().push(format!(
                        "More than one client connected to an installation. count: {}",
                        client.num_clients
                    ));
                }
            }
            (_, Event::ClientDropped) => {
                client.num_clients -= 1;
            }
            (Some(group_id), raw_event) => {
                {
                    let mut epochs = self.grouped_epochs.write();
                    if !epochs.contains_key(group_id) {
                        epochs.insert(group_id.to_string(), HashMap::new());
                    }
                }

                let mut group = client.group(group_id, &event);
                let mut group_state = group.new_event(&event);

                if let Ok(epoch) = ctx("epoch").and_then(|e| e.as_int()) {
                    // Reset the auth.
                    if group_state.epoch.is_some_and(|e| e != epoch) {
                        group_state.epoch_auth = None;
                    }
                    group_state.epoch = Some(epoch);
                }
                if let Ok(auth) = ctx("epoch_auth").and_then(|e| e.as_str()) {
                    group_state.epoch_auth = Some(auth.to_string());
                }
                if let Ok(cursor) = ctx("cursor").and_then(|e| e.as_int()) {
                    group_state.cursor = Some(cursor);
                }
                if let Ok(originator) = ctx("originator").and_then(|e| e.as_int()) {
                    group_state.originator = Some(originator);
                }

                match raw_event {
                    Event::CreatedDM => {
                        group_state.dm_target = Some(ctx("target_inbox")?.as_str()?.to_string());
                    }
                    Event::MLSGroupEpochUpdated => {
                        group_state.previous_epoch = Some(ctx("previous_epoch")?.as_int()?);
                        group_state.cursor = Some(ctx("cursor")?.as_int()?);
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
    pub groups: HashMap<String, Arc<RwLock<Group>>>,
    pub num_clients: isize,
    pub inst: String,
    pub inbox_id: String,
}

impl ClientState {
    fn new(inst: &str, inbox_id: &str, name: Option<String>) -> Self {
        Self {
            name,
            events: Vec::new(),
            groups: HashMap::default(),
            inst: inst.to_string(),
            inbox_id: inbox_id.to_string(),
            num_clients: 0,
        }
    }

    fn group(&mut self, group_id: &str, event: &Arc<LogEvent>) -> RwLockWriteGuard<'_, Group> {
        if !self.groups.contains_key(group_id) {
            let state = Group::new(event);
            self.groups.insert(group_id.to_string(), state.clone());
            return self.groups[group_id].write();
        }

        self.groups.get(group_id).unwrap().write()
    }

    fn sort(&self) {
        self.groups.values().for_each(|g| g.write().sort());
    }
}

pub struct Group {
    pub installation_id: String,
    pub states: Vec<Arc<Mutex<GroupState>>>,
}

impl Group {
    fn sort(&mut self) {
        self.states
            .sort_by(|a, b| a.lock().event.time.cmp(&b.lock().event.time));
    }
}

#[derive(Clone)]
pub struct GroupState {
    pub event: Arc<LogEvent>,
    pub dm_target: Option<InstallationId>,
    pub previous_epoch: Option<i64>,
    pub epoch: Option<i64>,
    pub epoch_auth: Option<String>,
    pub cursor: Option<i64>,
    pub originator: Option<i64>,
    pub members: HashMap<InstallationId, Weak<RwLock<ClientState>>>,
    pub problems: Vec<GroupStateProblem>,
}

#[derive(Clone)]
pub struct GroupStateProblem {
    pub description: String,
    pub severity: Severity,
}

#[derive(Clone)]
pub enum Severity {
    Error,
}

impl Group {
    fn new(event: &Arc<LogEvent>) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            installation_id: event.installation.clone(),
            states: vec![Arc::new(Mutex::new(GroupState::new(event)))],
        }))
    }
}

impl GroupState {
    fn new(event: &Arc<LogEvent>) -> Self {
        Self {
            event: event.clone(),
            dm_target: None,
            previous_epoch: None,
            epoch: None,
            epoch_auth: None,
            cursor: None,
            originator: None,
            members: HashMap::new(),
            problems: Vec::new(),
        }
    }
}

impl Group {
    fn new_event(&mut self, event: &Arc<LogEvent>) -> MutexGuard<'_, GroupState> {
        let last = self.states.last().expect("There should always be one");

        let new_state = GroupState {
            problems: vec![],
            event: event.clone(),
            ..last.lock().clone()
        };

        self.states.push(Arc::new(Mutex::new(new_state)));
        self.states.last().expect("Just pushed").lock()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::state::{LogEvent, LogState};
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

        let state = LogState::new();
        state.ingest_all(lines);

        let welcome_found = state.clients.read().iter().any(|(_inst_id, c)| {
            c.read()
                .events
                .iter()
                .any(|e| e.event == Event::ProcessedWelcome)
        });
        assert!(welcome_found);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_log_parse() {
        let line = r#"2026-02-12T21:04:02.542222Z  INFO sync_with_conn{self=Group { id: [0x52db...7067], created: [21:04:02], client: [dce8901bbeb060ab6eeb05fa52c27bd1232e9a68a2759cdab430d029854d225e], installation: [1919c3ba4b4ff946372cf004c718810bfb8747732397369c9ce67b40c0d8ca8e] } who=dce8901bbeb060ab6eeb05fa52c27bd1232e9a68a2759cdab430d029854d225e}:process_messages{who=dce8901bbeb060ab6eeb05fa52c27bd1232e9a68a2759cdab430d029854d225e}:process_message{trust_message_order=true envelope=GroupMessage { cursor [sid( 52773):oid(  0)], depends on                          , created at 21:04:02.492156, group 52dbb036fa67ad5b7c1f90f55f787067 }}: xmtp_mls::groups::mls_sync: ➣ Received staged commit. Merging and clearing any pending commits. {group_id: "52dbb036", sender_installation_id: "cbedac3e", msg_epoch: 1, epoch: 1, time: 1770930242542, inst: "1919c3ba"} inbox_id="dce8901bbeb060ab6eeb05fa52c27bd1232e9a68a2759cdab430d029854d225e" sender_inbox="46c640424325059475531d54205ac3f635d75125d79dd8cfe37e303bcbb24cdc" sender_installation_id=cbedac3e group_id=52dbb036 epoch=1 msg_epoch=1 msg_group_id=52dbb036 cursor=[sid( 52773):oid(  0)]"#;
        let mut line = line.split('\n').peekable();

        let event = LogEvent::from(&mut line)?;
        assert_eq!(event.event, Event::MLSReceivedStagedCommit);

        let group_id = event.context("group_id");
        assert!(group_id.is_some());
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
