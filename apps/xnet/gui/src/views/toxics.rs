//! Toxics management page — view rendering functions.
//!
//! Stateless render functions for the active toxics list panel.
//! Interactive elements (buttons) are rendered by `RootView` since they need
//! `cx.listener()`.

use gpui::{SharedString, div, prelude::*};

use crate::{state::ToxicInfo, theme, ui};

/// Renders the panel listing all currently active toxics.
pub fn render_active_toxics_panel(toxics: &[ToxicInfo]) -> impl IntoElement {
    let mut panel = ui::panel_container()
        .w_full()
        .child(ui::panel_title("Active Toxics", theme::accent_yellow()));

    if toxics.is_empty() {
        panel = panel.child(ui::empty_state("No toxics active"));
    } else {
        for toxic in toxics {
            panel = panel.child(render_toxic_row(toxic));
        }
    }

    panel
}

/// Renders a single toxic row with proxy name, type, stream, and parameters.
fn render_toxic_row(toxic: &ToxicInfo) -> impl IntoElement {
    let detail: SharedString = match toxic.toxic_type.as_str() {
        "latency" => format!(
            "{} | {} | {}ms latency, {}ms jitter ({}%)",
            toxic.proxy_name,
            toxic.stream,
            toxic.latency.unwrap_or(0),
            toxic.jitter.unwrap_or(0),
            (toxic.toxicity * 100.0) as u32,
        )
        .into(),
        other => format!(
            "{} | {} | {} ({}%)",
            toxic.proxy_name,
            toxic.stream,
            other,
            (toxic.toxicity * 100.0) as u32,
        )
        .into(),
    };

    ui::list_item_row(
        theme::accent_yellow(),
        div()
            .text_color(theme::text_primary())
            .text_xs()
            .child(detail),
    )
}
