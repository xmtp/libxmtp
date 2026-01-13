use crate::{AppWindow, ContextEntry, LogEntry, LogStream, state::log::LogFile};
use slint::{Color, Model, ModelRc, SharedString, VecModel, Weak};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::Path,
};

pub fn open_file_dialog(handle: Weak<AppWindow>) {
    std::thread::spawn(move || {
        use native_dialog::FileDialog;
        if let Ok(Some(path)) = FileDialog::new()
            .set_title("Open Log File")
            .show_open_single_file()
        {
            let path_str = path.to_string_lossy().to_string();
            slint::invoke_from_event_loop(move || {
                if let Some(ui) = handle.upgrade() {
                    ui.invoke_file_selected(path_str.into());
                }
            })
            .ok();
        }
    });
}

fn format_duration_ns(duration_ns: i64) -> String {
    if duration_ns < 0 {
        return String::new();
    }

    if duration_ns >= 1_000_000_000 {
        format!("+{:.2}s", duration_ns as f64 / 1_000_000_000.0)
    } else if duration_ns >= 1_000_000 {
        format!("+{:.2}ms", duration_ns as f64 / 1_000_000.0)
    } else if duration_ns >= 1_000 {
        format!("+{:.2}µs", duration_ns as f64 / 1_000.0)
    } else {
        format!("+{}ns", duration_ns)
    }
}

/// Generate a color from a string by hashing it
fn color_from_string(s: &str) -> Color {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    let hash = hasher.finish();

    // Use HSL with fixed saturation and lightness for nice pastel colors
    // Extract hue from hash (0-360)
    let hue = (hash % 360) as f32;
    let saturation = 0.65;
    let lightness = 0.55;

    // Convert HSL to RGB
    let (r, g, b) = hsl_to_rgb(hue, saturation, lightness);

    Color::from_rgb_u8(r, g, b)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

/// Intermediate struct that is Send-safe for passing stream data to the UI thread
struct StreamData {
    inbox: String,
    entries: Vec<EntryData>,
}

struct EntryData {
    event: String,
    duration_to_next: String,
    context: Vec<(String, String)>,
    group_id: Option<String>,
}

pub fn file_selected(handle: Weak<AppWindow>, path: impl AsRef<Path>) {
    let path = path.as_ref();
    tracing::info!("Selected logs file {path:?}");

    let log_file = match LogFile::load(path) {
        Ok(log) => log,
        Err(err) => {
            tracing::error!("Unable to open log {path:?} {err:?}");
            return;
        }
    };

    // Convert each inbox stream to a StreamData
    let streams: Vec<StreamData> = log_file
        .streams
        .into_iter()
        .map(|(inbox, events)| {
            // Collect timestamps for duration calculation
            let timestamps: Vec<i64> = events.iter().map(|e| e.timestamp()).collect();

            let entries: Vec<EntryData> = events
                .iter()
                .enumerate()
                .map(|(index, event)| {
                    let duration_to_next = if index + 1 < timestamps.len() {
                        let duration_ns = timestamps[index + 1] - timestamps[index];
                        format_duration_ns(duration_ns)
                    } else {
                        String::new()
                    };

                    EntryData {
                        event: event.event_name().to_string(),
                        duration_to_next,
                        context: event.context_entries(),
                        group_id: event.group_id().map(|s| s.to_string()),
                    }
                })
                .collect();

            StreamData { inbox, entries }
        })
        .collect();

    handle
        .upgrade_in_event_loop(move |ui| {
            // Build a color map for all group_ids
            let mut group_colors: HashMap<String, Color> = HashMap::new();

            // Convert StreamData to Slint LogStream (must happen in event loop due to ModelRc)
            let slint_streams: Vec<LogStream> = streams
                .into_iter()
                .map(|stream| {
                    let slint_entries: Vec<LogEntry> = stream
                        .entries
                        .into_iter()
                        .map(|e| {
                            // Get or create color for this group_id
                            let (group_color, has_group) = if let Some(ref gid) = e.group_id {
                                let color = group_colors
                                    .entry(gid.clone())
                                    .or_insert_with(|| color_from_string(gid))
                                    .clone();
                                (color, true)
                            } else {
                                (Color::from_rgb_u8(200, 200, 200), false)
                            };

                            // Convert context to Slint ContextEntry model
                            let context_entries: Vec<ContextEntry> = e
                                .context
                                .into_iter()
                                .map(|(key, value)| ContextEntry {
                                    key: SharedString::from(key),
                                    value: SharedString::from(value),
                                })
                                .collect();

                            LogEntry {
                                event: SharedString::from(e.event),
                                inbox: SharedString::from(&stream.inbox),
                                duration_to_next: SharedString::from(e.duration_to_next),
                                context: ModelRc::new(VecModel::from(context_entries)),
                                group_color,
                                has_group,
                            }
                        })
                        .collect();

                    let entries_model = ModelRc::new(VecModel::from(slint_entries));

                    LogStream {
                        inbox: SharedString::from(stream.inbox),
                        entries: entries_model,
                    }
                })
                .collect();

            // Get existing log streams and append the new ones
            let existing = ui.get_log_streams();
            let mut all_streams: Vec<LogStream> = existing.iter().collect();
            all_streams.extend(slint_streams);

            let model = ModelRc::new(VecModel::from(all_streams));
            ui.set_log_streams(model);
        })
        .ok();
}
