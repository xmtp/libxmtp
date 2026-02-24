pub mod assertions;
pub mod event;
pub mod state_or_event;
pub mod ui;
pub mod value;

use crate::{
    AppWindow,
    state::{
        assertions::{
            LogAssertion, account_for_drift::AccountForDrift, build_group_order::BuildGroupOrder,
            build_timeline::BuildTimeline, epoch_auth_consistency::EpochAuthConsistency,
            epoch_continuity::EpochContinuityAssertion,
        },
        state_or_event::StateOrEvent,
    },
};
use anyhow::{Context, Result};
pub use event::LogEvent;
use parking_lot::{Mutex, MutexGuard};
use slint::Weak as SlintWeak;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
};

/// Global counter for unique GroupState IDs
static GROUP_STATE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generate a new unique ID for a GroupState
fn next_group_state_id() -> u64 {
    GROUP_STATE_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}
pub use value::Value;
use xmtp_common::Event;

type InstallationId = String;
type GroupId = String;
type EpochNumber = i64;

pub struct State {
    pub sources: Mutex<HashMap<String, Vec<Arc<LogEvent>>>>,
    pub groups: Arc<Mutex<HashMap<GroupId, Arc<Mutex<Group>>>>>,

    pub group_order: Mutex<BTreeMap<i64, GroupId>>,
    pub grouped_epochs: Mutex<HashMap<GroupId, HashMap<InstallationId, Epochs>>>,
    pub timeline: Mutex<HashMap<GroupId, Vec<StateOrEvent>>>,

    pub clients: Mutex<HashMap<InstallationId, Arc<Mutex<ClientState>>>>,

    pub ui: Option<SlintWeak<AppWindow>>,
    /// Current page for events (0-indexed)
    pub events_page: AtomicU32,
    /// Current page for groups (0-indexed), shared between Epochs and Timeline tabs
    pub groups_page: AtomicU32,
}

#[derive(Default)]
pub struct Epochs {
    // These are important events without an associated group state
    // that we want to display in the epoch tab.
    pub outer_events: Arc<Mutex<Vec<Arc<LogEvent>>>>,
    pub epochs: BTreeMap<EpochNumber, Epoch>,
}

#[derive(Default)]
pub struct Epoch {
    pub states: Vec<Arc<Mutex<GroupState>>>,
}

pub struct ClientState {
    pub name: Option<String>,
    pub events: Vec<Arc<LogEvent>>,
    pub all_groups: Arc<Mutex<HashMap<GroupId, Arc<Mutex<Group>>>>>,
    pub groupa: HashMap<GroupId, Arc<Mutex<Group>>>,
    pub num_clients: isize,
    pub inst: String,
    pub inbox_id: String,
}

impl State {
    pub fn new(ui: Option<SlintWeak<AppWindow>>) -> Arc<Self> {
        Arc::new(Self {
            groups: Arc::default(),
            clients: Mutex::default(),
            group_order: Mutex::default(),
            grouped_epochs: Mutex::default(),
            sources: Mutex::default(),
            timeline: Mutex::default(),
            ui,
            events_page: AtomicU32::new(0),
            groups_page: AtomicU32::new(0),
        })
    }

    /// Set the events page and trigger a UI update
    pub fn set_events_page(self: &Arc<Self>, page: u32) {
        self.events_page.store(page, Ordering::Relaxed);
        self.clone().update_ui();
    }

    /// Set the groups page and trigger a UI update
    pub fn set_groups_page(self: &Arc<Self>, page: u32) {
        self.groups_page.store(page, Ordering::Relaxed);
        self.clone().update_ui();
    }

    /// Find a GroupState by its unique_id, narrowed by installation_id and group_id
    pub fn find_group_state_by_id(&self, group_id: &str, unique_id: u64) -> Option<GroupState> {
        let groups = self.groups.lock();
        let group = groups.get(group_id)?;
        let group = group.lock();
        for state in &group.states {
            let state = state.lock();
            if state.unique_id == unique_id {
                return Some(state.clone());
            }
        }
        None
    }

    pub fn add_source(self: &Arc<Self>, source: impl ToString, events: Vec<Arc<LogEvent>>) {
        self.sources.lock().insert(source.to_string(), events);
        self.rebuild();
    }

    pub fn remove_source(self: &Arc<Self>, source: &str) {
        self.sources.lock().remove(source);
        self.rebuild();
    }

    fn rebuild(self: &Arc<Self>) {
        // Perform a soft-reset.
        *self.grouped_epochs.lock() = HashMap::new();
        *self.timeline.lock() = HashMap::new();
        *self.clients.lock() = HashMap::new();

        {
            let sources = self.sources.lock();
            for (_source, events) in &*sources {
                for event in events {
                    if let Err(err) = self.ingest(event) {
                        tracing::error!("Event ingest error: {err}");
                    }
                }
            }
        }

        if let Err(err) = AccountForDrift::assert(self) {
            tracing::error!("Continuity error: {err}");
        };
        if let Err(err) = EpochContinuityAssertion::assert(self) {
            tracing::error!("Continuity error: {err}");
        };
        if let Err(err) = EpochAuthConsistency::assert(self) {
            tracing::error!("Epoch auth consistency error: {err}");
        };
        if let Err(err) = BuildTimeline::assert(self) {
            tracing::error!("Build timeline error: {err}");
        };
        if let Err(err) = BuildGroupOrder::assert(self) {
            tracing::error!("Build group order error: {err}");
        };

        // sort everything by time
        // self.clients.lock().values().for_each(|c| c.lock().sort());

        // update the ui
        self.clone().update_ui();
    }

    fn ingest(&self, event: &Arc<LogEvent>) -> Result<()> {
        let ctx = |key: &str| -> Result<&Value> {
            event
                .context(key)
                .with_context(|| format!("Missing context field {key}"))
        };

        let installation = &event.installation;
        let mut clients = self.clients.lock();
        let mut client = match event.event {
            Event::ClientCreated => {
                let inbox_id = ctx("inbox_id")?.as_str()?;
                clients.entry(installation.to_string()).or_insert_with(|| {
                    Arc::new(Mutex::new(ClientState::new(
                        &installation,
                        inbox_id,
                        None,
                        self.groups.clone(),
                    )))
                })
            }
            _ => clients.get_mut(installation).context("Missing client")?,
        }
        .lock();
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
                    let mut epochs = self.grouped_epochs.lock();
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
                    _ => {}
                }
            }
            _ => {}
        }

        client.events.push(event.clone());

        Ok(())
    }
}

impl ClientState {
    fn new(
        inst: &str,
        inbox_id: &str,
        name: Option<String>,
        all_groups: Arc<Mutex<HashMap<GroupId, Arc<Mutex<Group>>>>>,
    ) -> Self {
        Self {
            name,
            events: Vec::new(),
            inst: inst.to_string(),
            inbox_id: inbox_id.to_string(),
            num_clients: 0,
            all_groups,
            groupa: HashMap::default(),
        }
    }

    fn key(&self) -> String {
        if let Some(name) = &self.name {
            format!("{name}-{}", self.inst)
        } else {
            self.inst.clone()
        }
    }

    fn group(&mut self, group_id: &str, event: &Arc<LogEvent>) -> MutexGuard<'_, Group> {
        if !self.groupa.contains_key(group_id) {
            {
                let all_groups = self.all_groups.lock();
                if let Some(group) = all_groups.get(group_id) {
                    self.groupa.insert(group_id.to_string(), group.clone());
                } else {
                    let group = Group::new(event);
                    self.groupa.insert(group_id.to_string(), group.clone());
                }
            }
        }

        self.groupa.get(group_id).unwrap().lock()
    }
}

pub struct Group {
    pub has_errors: AtomicBool,
    pub installation_id: String,
    pub states: Vec<Arc<Mutex<GroupState>>>,
}

impl Group {
    fn new(event: &Arc<LogEvent>) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            installation_id: event.installation.clone(),
            states: vec![Arc::new(Mutex::new(GroupState::new(event)))],
            has_errors: AtomicBool::new(false),
        }))
    }

    fn sort(&mut self) {
        self.states
            .sort_by(|a, b| a.lock().event.time().cmp(&b.lock().event.time()));
    }
}

#[derive(Clone)]
pub struct GroupState {
    /// Globally unique ID for this GroupState instance
    pub unique_id: u64,
    pub event: Arc<LogEvent>,
    pub dm_target: Option<InstallationId>,
    pub epoch: Option<i64>,
    pub epoch_auth: Option<String>,
    pub cursor: Option<i64>,
    pub originator: Option<i64>,
    pub members: HashMap<InstallationId, Weak<Mutex<ClientState>>>,
}

impl GroupState {
    fn new(event: &Arc<LogEvent>) -> Self {
        Self {
            unique_id: next_group_state_id(),
            event: event.clone(),
            dm_target: None,
            epoch: None,
            epoch_auth: None,
            cursor: None,
            originator: None,
            members: HashMap::new(),
        }
    }
}

impl Group {
    fn new_event(&mut self, event: &Arc<LogEvent>) -> MutexGuard<'_, GroupState> {
        let last = self.states.last().expect("There should always be one");
        let last_guard = last.lock();

        let new_state = GroupState {
            unique_id: next_group_state_id(),
            event: event.clone(),
            dm_target: last_guard.dm_target.clone(),
            epoch: last_guard.epoch,
            epoch_auth: last_guard.epoch_auth.clone(),
            cursor: last_guard.cursor,
            originator: last_guard.originator,
            members: last_guard.members.clone(),
        };
        drop(last_guard);

        self.states.push(Arc::new(Mutex::new(new_state)));
        self.states.last().expect("Just pushed").lock()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::state::{LogEvent, State};
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

        let state = State::new(None);
        let events = LogEvent::parse(lines);
        state.add_source("anon", events);

        let welcome_found = state.clients.lock().iter().any(|(_inst_id, c)| {
            c.lock()
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

        let mut line_count: usize = 0;
        let event = LogEvent::from(&mut line, &mut line_count)?;
        assert_eq!(event.event, Event::MLSReceivedStagedCommit);

        let group_id = event.context("group_id");
        assert!(group_id.is_some());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_dm_created_with_unquoted_group_id() {
        let line = r#"2026-01-13T16:01:32.795843Z  INFO xmtp_mls::client: ➣ DM created. {group_id: 6dbafe8fc16699dfe3b59d60944150b3, target_inbox: "ab23790529e1fa52ed453e69d0d342f02bc8db8e2317f6229672dd0ca4f6d527", inbox: "2857d", timestamp: 1768320092795839419} group_id=6dbafe8fc16699dfe3b59d60944150b3 target_inbox="ab23790529e1fa52ed453e69d0d342f02bc8db8e2317f6229672dd0ca4f6d527" inbox=2857d"#;

        let mut line_count: usize = 0;
        let event = LogEvent::from(&mut line.split('\n').peekable(), &mut line_count)?;
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
