use crate::{
    AppWindow, UIContextEntry, UIEpochHeader, UIEvent, UIGroupRow, UIGroupState,
    UIInstallationCell, UIInstallationRow, UIStream, state::LogState,
    ui::file_open::color_from_string,
};
use slint::{ModelRc, SharedString, VecModel, Weak};
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
                        epoch_numbers.extend(epochs.keys());
                    }

                    // 1. Collect the union of all installation IDs across every epoch
                    let inst_ids: Vec<_> = inst_epochs_map.keys().cloned().collect();

                    // 2. Build epoch headers with max_states per epoch
                    let mut epoch_headers: Vec<UIEpochHeader> = Vec::new();
                    let mut max_states_per_epoch: Vec<i32> = Vec::new();

                    for epoch_number in &epoch_numbers {
                        let max_states = inst_epochs_map
                            .values()
                            .filter_map(|epochs| epochs.get(epoch_number).map(|e| e.states.len()))
                            .max()
                            .unwrap_or(1) as i32;

                        max_states_per_epoch.push(max_states);
                        epoch_headers.push(UIEpochHeader {
                            epoch_number: *epoch_number as i32,
                            max_states,
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

                        let mut cells: Vec<UIInstallationCell> = Vec::new();

                        for epoch_number in &epoch_numbers {
                            let ui_states: Vec<UIGroupState> = if let Some(epoch) = inst_epochs_map
                                .get(inst_id)
                                .and_then(|epochs| epochs.get(epoch_number))
                            {
                                epoch
                                    .states
                                    .iter()
                                    .map(|state_arc| {
                                        let state = state_arc.read();

                                        let context_entries: Vec<UIContextEntry> = state
                                            .event
                                            .ui_context_entries()
                                            .into_iter()
                                            .filter(|e| e.key != "group_id")
                                            .collect();

                                        let problem_strings: Vec<SharedString> = state
                                            .problems
                                            .iter()
                                            .map(|p| SharedString::from(&p.description))
                                            .collect();

                                        UIGroupState {
                                            installation_id: SharedString::from(inst_id),
                                            installation_name: SharedString::from(&inst_name),
                                            msg: SharedString::from(state.event.msg),
                                            icon: SharedString::from(state.event.icon),
                                            epoch: state.epoch.unwrap_or(*epoch_number) as i32,
                                            previous_epoch: state.previous_epoch.unwrap_or(0)
                                                as i32,
                                            has_previous_epoch: state.previous_epoch.is_some(),
                                            problem_count: state.problems.len() as i32,
                                            problems: ModelRc::new(VecModel::from(problem_strings)),
                                            context: ModelRc::new(VecModel::from(context_entries)),
                                        }
                                    })
                                    .collect()
                            } else {
                                // This installation has no states in this epoch — empty cell
                                Vec::new()
                            };

                            let state_count = ui_states.len() as i32;

                            cells.push(UIInstallationCell {
                                states: ModelRc::new(VecModel::from(ui_states)),
                                state_count,
                            });
                        }

                        installation_rows.push(UIInstallationRow {
                            installation_id: SharedString::from(inst_id.as_str()),
                            installation_name: SharedString::from(&display_name),
                            installation_color: inst_color,
                            cells: ModelRc::new(VecModel::from(cells)),
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
