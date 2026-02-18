use crate::{
    AppWindow, UIEpochHeader, UIEvent, UIGroupRow, UIGroupState, UIGroupStateDetail,
    UIInstallationCell, UIInstallationRow, UIStream,
    state::{GroupState, LogEvent, LogState},
    ui::file_open::color_from_string,
};
use slint::{Color, ModelRc, SharedString, VecModel, Weak};
use std::{collections::BTreeSet, sync::Arc};

fn short_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}…{}", &id[..6], &id[id.len() - 6..])
    } else {
        id.to_string()
    }
}

impl LogState {
    /// Look up a friendly name for an installation id from the clients map.
    fn installation_name(&self, installation_id: &str) -> String {
        let clients = self.clients.read();
        if let Some(client) = clients.get(installation_id) {
            let client = client.read();
            if let Some(ref name) = client.name {
                return name.clone();
            }
        }
        String::new()
    }

    pub fn update_ui(self: Arc<Self>, ui: &Weak<AppWindow>) {
        let _ = ui
            .upgrade_in_event_loop(move |ui| {
                // ─── Events Tab ───
                let mut streams = Vec::new();
                for (inst, client) in &*self.clients.read() {
                    let client = client.read();
                    let mut stream = Vec::new();

                    for event in &client.events {
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

                let mut group_rows: Vec<UIGroupRow> = Vec::new();

                let grouped_epochs = self.grouped_epochs.read();
                let mut group_ids: Vec<&String> = grouped_epochs.keys().collect();
                group_ids.sort();

                for group_id in group_ids {
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

                        let outer_events_guard = epochs.outer_events.read();
                        let mut outer_events = outer_events_guard.iter().peekable();

                        for (i, epoch_number) in epoch_numbers.iter().enumerate() {
                            let mut ui_states = vec![];

                            // First, drain any outer_events that come before this epoch
                            if let Some(epoch) = epochs.epochs.get(epoch_number) {
                                let mut states = epoch.states.iter().peekable();

                                loop {
                                    let cell = match (states.peek(), outer_events.peek()) {
                                        (Some(state), Some(event))
                                            if event.time < state.lock().event.time =>
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

                let epoch_groups = ModelRc::new(VecModel::from(group_rows));
                ui.set_epoch_groups(epoch_groups);
            })
            .inspect_err(|e| tracing::error!("{e:?}"));
    }
}

impl LogEvent {
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
