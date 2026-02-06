use crate::{AppWindow, state::LogState};
use slint::{Color, Weak};
use std::{
    collections::HashMap,
    fs::read_to_string,
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
pub fn color_from_string(s: &str) -> Color {
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
    installation: String,
    entries: Vec<EntryData>,
}

struct EntryData {
    event: String,
    duration_to_next: String,
    context: HashMap<String, String>,
    group_id: Option<String>,
}

pub fn file_selected(handle: Weak<AppWindow>, path: impl AsRef<Path>) {
    let path = path.as_ref();
    tracing::info!("Selected logs file {path:?}");

    // Load the entire file in memory for now. We can optimize later.
    let log_file = match read_to_string(path) {
        Ok(str) => str,
        Err(err) => {
            tracing::error!("Unable to open log {path:?} {err:?}");
            return;
        }
    };

    let lines = log_file.split('\n').peekable();
    let state = LogState::build(lines);
    state.update_ui(&handle);
}
