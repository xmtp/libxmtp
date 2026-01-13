//! Log panel view for displaying application logs.

use std::sync::Arc;

use gpui::{ScrollHandle, SharedString, div, prelude::*, px};
use gpui_component::Sizable;
use gpui_component::alert::Alert;

use crate::{theme, ui};

/// Renders the log panel showing application log lines.
///
/// The panel is scrollable and tracked by the provided [`ScrollHandle`] so the
/// caller can programmatically scroll to the bottom after new lines arrive.
pub fn render_log_panel(log_lines: &[Arc<str>], scroll_handle: &ScrollHandle) -> impl IntoElement {
    let mut lines = div().flex().flex_col().gap(px(2.0));

    for line in log_lines {
        lines = lines.child(render_log_line(line));
    }

    ui::panel_container()
        .w_full()
        .h_full()
        .child(div().text_color(theme::text_muted()).text_xs().child("Log"))
        .child(
            div()
                .id("log-scroll")
                .flex_grow()
                .overflow_y_scroll()
                .track_scroll(scroll_handle)
                .child(lines),
        )
}

/// Renders a single log line with a colored level prefix.
///
/// Log lines have the format `[LEVEL] target: message`. The `[LEVEL]` portion
/// is colored based on the log level while the rest uses the default text color.
fn render_log_line(line: &Arc<str>) -> impl IntoElement {
    // Try to split at the closing bracket to extract the level tag.
    if let Some(bracket_end) = line.find(']') {
        let level_tag = &line[..=bracket_end];
        let rest: SharedString = line[bracket_end + 1..].to_string().into();

        let level_color = if level_tag.contains("ERROR") {
            theme::accent_red()
        } else if level_tag.contains("WARN") {
            theme::accent_yellow()
        } else if level_tag.contains("INFO") {
            theme::accent_green()
        } else if level_tag.contains("DEBUG") {
            theme::accent_blue()
        } else if level_tag.contains("TRACE") {
            theme::accent_mauve()
        } else {
            theme::text_secondary()
        };

        let tag: SharedString = level_tag.to_string().into();

        div()
            .flex()
            .flex_row()
            .text_xs()
            .child(div().text_color(level_color).child(tag))
            .child(div().text_color(theme::text_secondary()).child(rest))
    } else {
        let s: SharedString = SharedString::from(line.clone());
        div()
            .flex()
            .flex_row()
            .text_xs()
            .child(div().text_color(theme::text_secondary()).child(s))
    }
}

/// Renders an error bar if an error message is present.
pub fn render_error_bar(last_error: &Option<String>) -> gpui::AnyElement {
    if let Some(err) = last_error {
        Alert::error("error-bar", err.clone())
            .banner()
            .small()
            .into_any_element()
    } else {
        div().into_any_element()
    }
}
