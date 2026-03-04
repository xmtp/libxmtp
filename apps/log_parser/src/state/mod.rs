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
            flag_groups_for_errors::FlagGroupsForErrors,
        },
        state_or_event::StateOrEvent,
    },
};
use anyhow::{Context, Result};
pub use event::LogEvent;
use parking_lot::{Mutex, MutexGuard};
use slint::Weak as SlintWeak;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
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
    // State
    pub sources: Mutex<HashMap<String, Vec<Arc<LogEvent>>>>,
    pub groups: Arc<Mutex<HashMap<GroupId, Arc<Mutex<Group>>>>>,

    // Meta state
    pub group_order: Mutex<BTreeMap<i64, GroupId>>,
    pub grouped_epochs: Mutex<HashMap<GroupId, HashMap<InstallationId, Epochs>>>,
    pub clients: Mutex<HashMap<InstallationId, Arc<Mutex<ClientState>>>>,

    // UI
    pub ui: Option<SlintWeak<AppWindow>>,
    /// Current page for events (0-indexed)
    pub events_page: AtomicU32,
    /// Current page for groups (0-indexed), shared between Epochs and Timeline tabs
    pub groups_page: AtomicU32,
    /// Filter to show only groups with errors
    pub show_errors_only: AtomicBool,
    /// Set of group IDs to focus on (when non-empty, only these groups are shown)
    pub focused_group_ids: Mutex<HashSet<String>>,
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
    pub groups: HashMap<GroupId, Arc<Mutex<Group>>>,
    pub num_clients: isize,
}

impl State {
    pub fn new(ui: Option<SlintWeak<AppWindow>>) -> Arc<Self> {
        Arc::new(Self {
            groups: Arc::default(),
            clients: Mutex::default(),
            group_order: Mutex::default(),
            grouped_epochs: Mutex::default(),
            sources: Mutex::default(),
            ui,
            events_page: AtomicU32::new(0),
            groups_page: AtomicU32::new(0),
            show_errors_only: AtomicBool::new(false),
            focused_group_ids: Mutex::new(HashSet::new()),
        })
    }

    /// Set the show_errors_only filter and trigger a UI update
    /// When enabled, focuses on all groups with errors
    /// When disabled, clears all focused groups
    pub fn set_show_errors_only(self: &Arc<Self>, show_errors_only: bool) {
        self.show_errors_only
            .store(show_errors_only, Ordering::Relaxed);

        // Update focused groups based on the filter
        let mut focused = self.focused_group_ids.lock();
        if show_errors_only {
            // Populate with all groups that have errors
            let groups = self.groups.lock();
            focused.clear();
            for (group_id, group) in groups.iter() {
                if group.lock().has_errors.load(Ordering::Relaxed) {
                    focused.insert(group_id.clone());
                }
            }
        } else {
            // Clear all focused groups
            focused.clear();
        }
        drop(focused);

        // Reset to first page when filter changes
        self.groups_page.store(0, Ordering::Relaxed);
        self.clone().update_ui();
    }

    /// Add a group ID to the focused set
    pub fn focus_group(self: &Arc<Self>, group_id: String) {
        self.focused_group_ids.lock().insert(group_id);
        // Reset to first page when focus changes
        self.groups_page.store(0, Ordering::Relaxed);
        self.clone().update_ui();
    }

    /// Remove a group ID from the focused set
    pub fn unfocus_group(self: &Arc<Self>, group_id: &str) {
        self.focused_group_ids.lock().remove(group_id);
        // Reset to first page when focus changes
        self.groups_page.store(0, Ordering::Relaxed);
        self.clone().update_ui();
    }

    /// Clear all focused groups
    pub fn clear_focused_groups(self: &Arc<Self>) {
        self.focused_group_ids.lock().clear();
        // Reset to first page when focus changes
        self.groups_page.store(0, Ordering::Relaxed);
        self.clone().update_ui();
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
        for states_by_installation in &group.states {
            for state in states_by_installation.values() {
                let state = state.lock();
                if state.unique_id == unique_id {
                    return Some(state.clone());
                }
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
        *self.clients.lock() = HashMap::new();
        *self.groups.lock() = HashMap::new();

        {
            let sources = self.sources.lock();
            for events in sources.values() {
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
        if let Err(err) = FlagGroupsForErrors::assert(self) {
            tracing::error!("Flag groups for errors error: {err}");
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
            Event::ClientCreated => clients.entry(installation.to_string()).or_insert_with(|| {
                Arc::new(Mutex::new(ClientState::new(None, self.groups.clone())))
            }),
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

                let mut group = client.group(group_id, event);
                let mut group_state = group.new_event(event);

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

                if raw_event == Event::CreatedDM {
                    group_state.dm_target = Some(ctx("target_inbox")?.as_str()?.to_string());
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
        name: Option<String>,
        all_groups: Arc<Mutex<HashMap<GroupId, Arc<Mutex<Group>>>>>,
    ) -> Self {
        Self {
            name,
            events: Vec::new(),

            num_clients: 0,
            all_groups,
            groups: HashMap::default(),
        }
    }

    fn group(&mut self, group_id: &str, event: &Arc<LogEvent>) -> MutexGuard<'_, Group> {
        if !self.groups.contains_key(group_id) {
            {
                let mut all_groups = self.all_groups.lock();
                if let Some(group) = all_groups.get(group_id) {
                    self.groups.insert(group_id.to_string(), group.clone());
                } else {
                    let group = Group::new(event);
                    all_groups.insert(group_id.to_string(), group.clone());
                    self.groups.insert(group_id.to_string(), group.clone());
                }
            }
        }

        self.groups.get(group_id).unwrap().lock()
    }
}

pub struct Group {
    pub has_errors: AtomicBool,
    pub timeline: Vec<StateOrEvent>,
    pub states: Vec<HashMap<InstallationId, Arc<Mutex<GroupState>>>>,
}

impl Group {
    fn new(event: &Arc<LogEvent>) -> Arc<Mutex<Self>> {
        let mut states_map = HashMap::new();
        states_map.insert(
            event.installation.clone(),
            Arc::new(Mutex::new(GroupState::new(event))),
        );
        Arc::new(Mutex::new(Self {
            states: vec![states_map],
            has_errors: AtomicBool::new(false),
            timeline: Vec::new(),
        }))
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
        let installation_id = event.installation.clone();

        // Find the last state for this installation to copy from
        let last_state_for_installation = self
            .states
            .iter()
            .rev()
            .find_map(|states_map| states_map.get(&installation_id));

        let new_state = if let Some(last) = last_state_for_installation {
            let last_guard = last.lock();
            GroupState {
                unique_id: next_group_state_id(),
                event: event.clone(),
                dm_target: last_guard.dm_target.clone(),
                epoch: last_guard.epoch,
                epoch_auth: last_guard.epoch_auth.clone(),
                cursor: last_guard.cursor,
                originator: last_guard.originator,
                members: last_guard.members.clone(),
            }
        } else {
            GroupState::new(event)
        };

        let new_state_arc = Arc::new(Mutex::new(new_state));
        let mut new_states_map = HashMap::new();
        new_states_map.insert(installation_id.clone(), new_state_arc);
        self.states.push(new_states_map);

        self.states
            .last()
            .expect("Just pushed")
            .get(&installation_id)
            .expect("Just inserted")
            .lock()
    }
}
