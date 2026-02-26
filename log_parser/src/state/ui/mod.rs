use crate::{
    UIEpochHeader, UIEvent, UIGroupRow, UIGroupState, UIGroupStateDetail, UIInstallationCell,
    UIInstallationRow, UISource, UIStream, UITimelineEntry, UITimelineGroup,
    UITimelineInstallationRow,
    state::{GroupState, LogEvent, State, StateOrEvent},
    ui::file_open::color_from_string,
};
use slint::{Color, ModelRc, SharedString, VecModel};
use std::collections::HashMap as StdHashMap;
use std::sync::atomic::Ordering;
use std::{collections::BTreeSet, sync::Arc};

/// Maximum number of events or groups to display per page in the UI
const PAGE_SIZE: usize = 1000;

fn short_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}…{}", &id[..6], &id[id.len() - 6..])
    } else {
        id.to_string()
    }
}

impl State {
    /// Look up a friendly name for an installation id from the clients map.
    fn installation_name(&self, installation_id: &str) -> String {
        let clients = self.clients.lock();
        if let Some(client) = clients.get(installation_id) {
            let client = client.lock();
            if let Some(ref name) = client.name {
                return name.clone();
            }
        }
        String::new()
    }

    pub fn update_ui(self: Arc<Self>) {
        let Some(ui) = self.ui.clone() else {
            return;
        };

        let _ = ui
            .upgrade_in_event_loop(move |ui| {
                // ─── Events Tab ───
                let events_page = self.events_page.load(Ordering::Relaxed) as usize;
                let mut streams = Vec::new();
                let mut max_events_count: usize = 0;

                for (inst, client) in &*self.clients.lock() {
                    let client = client.lock();
                    let total_events = client.events.len();
                    max_events_count = max_events_count.max(total_events);

                    let mut stream = Vec::new();
                    let start = events_page * PAGE_SIZE;
                    let end = ((events_page + 1) * PAGE_SIZE).min(total_events);

                    for event in client.events.iter().skip(start).take(end - start) {
                        let color = event.ui_group_color();
                        stream.push(UIEvent {
                            event: SharedString::from(event.msg),
                            icon: SharedString::from(event.icon),
                            inst: SharedString::from(&event.installation),
                            context: ModelRc::new(VecModel::from(event.ui_context_entries())),
                            has_group: color.is_some(),
                            group_color: color.unwrap_or_default(),
                        });
                    }

                    let label = if let Some(ref name) = client.name {
                        format!("{name} ({inst})")
                    } else {
                        inst.clone()
                    };

                    streams.push(UIStream {
                        inst: SharedString::from(&label),
                        entries: ModelRc::new(VecModel::from(stream)),
                    });
                }

                // Calculate total pages for events
                let events_total_pages = if max_events_count == 0 {
                    1
                } else {
                    (max_events_count + PAGE_SIZE - 1) / PAGE_SIZE
                };
                ui.set_events_page(events_page as i32);
                ui.set_events_total_pages(events_total_pages as i32);

                let streams = ModelRc::new(VecModel::from(streams));
                ui.set_log_streams(streams);

                // ─── Epochs Tab ───
                // Transform: grouped_epochs (group → epoch → installation → [states])
                // Into:      UIGroupRow    (group → [epoch_headers], [installation_rows → [cells]])
                //
                // Each group becomes a table where:
                //   - Columns = epochs (sorted by epoch number)
                //   - Rows    = installations (union of all installations across all epochs)
                //   - Cells   = the group states for that (installation, epoch) pair
                //
                // Each epoch column header carries `max_states`: the maximum number of
                // states any single installation has in that epoch. This lets the UI
                // allocate a consistent column width so cells align across rows.

                let groups_page = self.groups_page.load(Ordering::Relaxed) as usize;
                let mut group_rows: Vec<UIGroupRow> = Vec::new();

                let grouped_epochs = self.grouped_epochs.lock();
                let mut group_ids: Vec<&String> = grouped_epochs.keys().collect();
                group_ids.sort();

                let total_groups = group_ids.len();
                let groups_total_pages = if total_groups == 0 {
                    1
                } else {
                    (total_groups + PAGE_SIZE - 1) / PAGE_SIZE
                };

                let start = groups_page * PAGE_SIZE;
                let end = ((groups_page + 1) * PAGE_SIZE).min(total_groups);

                for group_id in group_ids.into_iter().skip(start).take(end - start) {
                    let inst_epochs_map = &grouped_epochs[group_id];

                    // Sort epochs by epoch number
                    let mut epoch_numbers = BTreeSet::new();
                    for (_inst, epochs) in inst_epochs_map {
                        epoch_numbers.extend(epochs.epochs.keys());
                    }

                    // 1. Collect the union of all installation IDs across every epoch
                    let inst_ids: Vec<_> = inst_epochs_map.keys().cloned().collect();

                    // 2. Build epoch headers with max_states per epoch
                    let mut epoch_headers: Vec<UIEpochHeader> = Vec::new();

                    for epoch_number in &epoch_numbers {
                        epoch_headers.push(UIEpochHeader {
                            epoch_number: *epoch_number as i32,
                            max_states: 0, // this is set later
                        });
                    }

                    // 3. Build installation rows — one row per installation,
                    //    one cell per epoch (in the same order as epoch_headers)
                    let mut installation_rows: Vec<UIInstallationRow> = Vec::new();

                    for inst_id in &inst_ids {
                        let inst_name = self.installation_name(inst_id);
                        let inst_color = color_from_string(inst_id);

                        let display_name = if inst_name.is_empty() {
                            short_id(inst_id)
                        } else {
                            format!("{inst_name} ({})", short_id(inst_id))
                        };

                        let mut installation_cells: Vec<UIInstallationCell> = Vec::new();
                        let Some(epochs) = inst_epochs_map.get(inst_id) else {
                            installation_rows.push(UIInstallationRow {
                                installation_id: SharedString::from(inst_id.as_str()),
                                installation_name: SharedString::from(&display_name),
                                installation_color: inst_color,
                                cells: ModelRc::new(VecModel::from(vec![])),
                            });
                            continue;
                        };

                        let outer_events_guard = epochs.outer_events.lock();
                        let mut outer_events = outer_events_guard.iter().peekable();

                        for (i, epoch_number) in epoch_numbers.iter().enumerate() {
                            let mut ui_states = vec![];

                            // First, drain any outer_events that come before this epoch
                            if let Some(epoch) = epochs.epochs.get(epoch_number) {
                                let mut states = epoch.states.iter().peekable();

                                loop {
                                    let cell = match (states.peek(), outer_events.peek()) {
                                        (Some(state), Some(event))
                                            if event.time() < state.lock().event.time() =>
                                        {
                                            let event = outer_events.next().unwrap();
                                            event.ui_group_state()
                                        }
                                        (Some(_state), _) => {
                                            let state = states.next().unwrap();
                                            state.lock().ui_group_state()
                                        }
                                        // Drain the outer events if we're in the last epoch
                                        (None, Some(_event)) if i + 1 == epoch_numbers.len() => {
                                            let event = outer_events.next().unwrap();
                                            event.ui_group_state()
                                        }
                                        _ => break,
                                    };
                                    ui_states.push(cell);
                                }
                            } else {
                                // No epoch data, but still drain outer events if this is the last epoch
                                if i + 1 == epoch_numbers.len() {
                                    while let Some(event) = outer_events.next() {
                                        ui_states.push(event.ui_group_state());
                                    }
                                }
                            }

                            let state_count = ui_states.len() as i32;
                            installation_cells.push(UIInstallationCell {
                                state_count,
                                states: ModelRc::new(VecModel::from(ui_states)),
                            });

                            epoch_headers[i].max_states =
                                epoch_headers[i].max_states.max(state_count);
                        }

                        installation_rows.push(UIInstallationRow {
                            installation_id: SharedString::from(inst_id.as_str()),
                            installation_name: SharedString::from(&display_name),
                            installation_color: inst_color,
                            cells: ModelRc::new(VecModel::from(installation_cells)),
                        });
                    }

                    let group_color = color_from_string(group_id);

                    group_rows.push(UIGroupRow {
                        group_id: SharedString::from(group_id.as_str()),
                        group_id_short: SharedString::from(short_id(group_id)),
                        group_color,
                        epoch_headers: ModelRc::new(VecModel::from(epoch_headers)),
                        installation_rows: ModelRc::new(VecModel::from(installation_rows)),
                    });
                }

                // Set pagination state for groups
                ui.set_groups_page(groups_page as i32);
                ui.set_groups_total_pages(groups_total_pages as i32);

                let epoch_groups = ModelRc::new(VecModel::from(group_rows));
                ui.set_epoch_groups(epoch_groups);

                // ─── Timeline Tab ───
                // Transform: timeline (group → [StateOrEvent])
                // Into:      UITimelineGroup (group → [UITimelineInstallationRow])
                //
                // Each group has installation rows, each with chronologically sorted entries

                let mut timeline_groups: Vec<UITimelineGroup> = Vec::new();

                let groups = self.groups.lock();
                let mut timeline_group_ids: Vec<&String> = groups.keys().collect();
                timeline_group_ids.sort();

                // Use the same pagination as epochs tab
                let start = groups_page * PAGE_SIZE;
                let end = ((groups_page + 1) * PAGE_SIZE).min(timeline_group_ids.len());

                for group_id in timeline_group_ids.into_iter().skip(start).take(end - start) {
                    let group = groups[group_id].lock();
                    let entries = &group.timeline;

                    // First pass: collect all unique timestamps to create time slots
                    // Each unique timestamp becomes a slot index
                    let mut all_timestamps: Vec<i64> = entries
                        .iter()
                        .map(|e| match e {
                            StateOrEvent::State(s) => s.lock().event.time(),
                            StateOrEvent::Event(e) => e.time(),
                        })
                        .collect();
                    all_timestamps.sort();
                    all_timestamps.dedup();

                    // Create a map from timestamp to slot index
                    let timestamp_to_slot: StdHashMap<i64, i32> = all_timestamps
                        .iter()
                        .enumerate()
                        .map(|(idx, &ts)| (ts, idx as i32))
                        .collect();

                    let total_slots = all_timestamps.len() as i32;

                    // Group entries by installation with slot indices
                    let mut entries_by_inst: StdHashMap<String, Vec<UITimelineEntry>> =
                        StdHashMap::new();
                    let mut total_entries = 0;

                    for entry in entries {
                        let (inst_id, ui_entry) = match entry {
                            StateOrEvent::State(state) => {
                                let state = state.lock();
                                let inst_id = state.event.installation.clone();
                                let inst_name = self.installation_name(&inst_id);
                                let display_name = if inst_name.is_empty() {
                                    short_id(&inst_id)
                                } else {
                                    inst_name.clone()
                                };
                                let slot_index =
                                    *timestamp_to_slot.get(&state.event.time()).unwrap_or(&0);
                                (
                                    inst_id,
                                    state.ui_timeline_entry(
                                        &state.event.installation,
                                        &display_name,
                                        slot_index,
                                    ),
                                )
                            }
                            StateOrEvent::Event(event) => {
                                let inst_id = event.installation.clone();
                                let inst_name = self.installation_name(&inst_id);
                                let display_name = if inst_name.is_empty() {
                                    short_id(&inst_id)
                                } else {
                                    inst_name.clone()
                                };
                                let slot_index =
                                    *timestamp_to_slot.get(&event.time()).unwrap_or(&0);
                                (
                                    inst_id,
                                    event.ui_timeline_entry(
                                        &event.installation,
                                        &display_name,
                                        slot_index,
                                    ),
                                )
                            }
                        };
                        entries_by_inst.entry(inst_id).or_default().push(ui_entry);
                        total_entries += 1;
                    }

                    // Build installation rows
                    let mut installation_rows: Vec<UITimelineInstallationRow> = Vec::new();
                    let mut inst_ids: Vec<String> = entries_by_inst.keys().cloned().collect();
                    inst_ids.sort();

                    for inst_id in inst_ids {
                        let inst_name = self.installation_name(&inst_id);
                        let display_name = if inst_name.is_empty() {
                            short_id(&inst_id)
                        } else {
                            format!("{inst_name} ({})", short_id(&inst_id))
                        };
                        let inst_color = color_from_string(&inst_id);
                        let inst_entries = entries_by_inst.remove(&inst_id).unwrap_or_default();

                        installation_rows.push(UITimelineInstallationRow {
                            installation_id: SharedString::from(inst_id.as_str()),
                            installation_name: SharedString::from(&display_name),
                            installation_color: inst_color,
                            entries: ModelRc::new(VecModel::from(inst_entries)),
                        });
                    }

                    let group_color = color_from_string(group_id);

                    timeline_groups.push(UITimelineGroup {
                        group_id: SharedString::from(group_id.as_str()),
                        group_id_short: SharedString::from(short_id(group_id)),
                        group_color,
                        installation_rows: ModelRc::new(VecModel::from(installation_rows)),
                        total_entries: total_entries as i32,
                        total_slots,
                    });
                }

                let timeline_groups = ModelRc::new(VecModel::from(timeline_groups));
                ui.set_timeline_groups(timeline_groups);

                // ─── Sources Tab ───
                // Transform: sources HashMap<String, Vec<Arc<LogEvent>>>
                // Into:      [UISource] with name and event count

                let mut ui_sources: Vec<UISource> = Vec::new();
                let sources = self.sources.lock();
                let mut source_names: Vec<&String> = sources.keys().collect();
                source_names.sort();

                for source_name in source_names {
                    let events = &sources[source_name];
                    ui_sources.push(UISource {
                        name: SharedString::from(source_name.as_str()),
                        event_count: events.len() as i32,
                    });
                }

                let sources_model = ModelRc::new(VecModel::from(ui_sources));
                ui.set_sources(sources_model);
            })
            .inspect_err(|e| tracing::error!("{e:?}"));
    }
}

impl LogEvent {
    /// Create a timeline entry UI representation of this LogEvent
    fn ui_timeline_entry(
        &self,
        installation_id: &str,
        installation_name: &str,
        slot_index: i32,
    ) -> UITimelineEntry {
        let problem_strings: Vec<SharedString> = self
            .problems
            .lock()
            .iter()
            .map(|p| SharedString::from(p))
            .collect();

        UITimelineEntry {
            unique_id: 0, // LogEvents don't have unique IDs
            msg: SharedString::from(self.msg),
            icon: SharedString::from(self.icon),
            installation_id: SharedString::from(installation_id),
            installation_name: SharedString::from(installation_name),
            installation_color: color_from_string(installation_id),
            epoch: -1,
            timestamp: self.time() as i32,
            slot_index,
            line_number: self.line_number as i32,
            problems: ModelRc::new(VecModel::from(problem_strings)),
            context: ModelRc::new(VecModel::from(self.ui_context_entries())),
            intermediate: SharedString::from(&self.intermediate),
        }
    }

    fn ui_group_state(&self) -> UIGroupState {
        let problem_strings: Vec<SharedString> = self
            .problems
            .lock()
            .iter()
            .map(|p| SharedString::from(p))
            .collect();

        UIGroupState {
            unique_id: 0, // LogEvents don't have unique IDs, only GroupStates do
            msg: SharedString::from(self.msg),
            icon: SharedString::from(self.icon),
            context: ModelRc::new(VecModel::from(self.ui_context_entries())),
            intermediate: SharedString::from(&self.intermediate),
            epoch: -1,
            problems: ModelRc::new(VecModel::from(problem_strings)),
            background: Color::from_rgb_u8(211, 211, 211),
            line_number: self.line_number as i32,
        }
    }
}

impl GroupState {
    fn ui_group_state(&self) -> UIGroupState {
        let problem_strings: Vec<SharedString> = self
            .event
            .problems
            .lock()
            .iter()
            .map(|p| SharedString::from(p))
            .collect();

        UIGroupState {
            unique_id: self.unique_id as i32,
            msg: SharedString::from(self.event.msg),
            icon: SharedString::from(self.event.icon),
            epoch: self.epoch.unwrap_or(-1) as i32,
            problems: ModelRc::new(VecModel::from(problem_strings)),
            context: ModelRc::new(VecModel::from(self.event.ui_context_entries())),
            intermediate: SharedString::from(&self.event.intermediate),
            background: Color::from_rgb_u8(255, 255, 255),
            line_number: self.event.line_number as i32,
        }
    }

    /// Create a timeline entry UI representation of this GroupState
    pub fn ui_timeline_entry(
        &self,
        installation_id: &str,
        installation_name: &str,
        slot_index: i32,
    ) -> UITimelineEntry {
        let problem_strings: Vec<SharedString> = self
            .event
            .problems
            .lock()
            .iter()
            .map(|p| SharedString::from(p))
            .collect();

        UITimelineEntry {
            unique_id: self.unique_id as i32,
            msg: SharedString::from(self.event.msg),
            icon: SharedString::from(self.event.icon),
            installation_id: SharedString::from(installation_id),
            installation_name: SharedString::from(installation_name),
            installation_color: color_from_string(installation_id),
            epoch: self.epoch.unwrap_or(-1) as i32,
            timestamp: self.event.time() as i32,
            slot_index,
            line_number: self.event.line_number as i32,
            problems: ModelRc::new(VecModel::from(problem_strings)),
            context: ModelRc::new(VecModel::from(self.event.ui_context_entries())),
            intermediate: SharedString::from(&self.event.intermediate),
        }
    }

    /// Create a detailed UI representation of this GroupState for the detail panel
    pub fn ui_group_state_detail(&self, installation_id: &str) -> UIGroupStateDetail {
        let problem_strings: Vec<SharedString> = self
            .event
            .problems
            .lock()
            .iter()
            .map(|p| SharedString::from(p))
            .collect();

        UIGroupStateDetail {
            unique_id: self.unique_id as i32,
            installation_id: SharedString::from(installation_id),
            msg: SharedString::from(self.event.msg),
            icon: SharedString::from(self.event.icon),
            epoch: self.epoch.unwrap_or(-1) as i32,
            epoch_auth: SharedString::from(self.epoch_auth.as_deref().unwrap_or("")),
            cursor: self.cursor.unwrap_or(-1) as i32,
            originator: self.originator.unwrap_or(-1) as i32,
            dm_target: SharedString::from(self.dm_target.as_deref().unwrap_or("")),
            line_number: self.event.line_number as i32,
            problems: ModelRc::new(VecModel::from(problem_strings)),
            context: ModelRc::new(VecModel::from(self.event.ui_context_entries())),
            intermediate: SharedString::from(&self.event.intermediate),
        }
    }
}
