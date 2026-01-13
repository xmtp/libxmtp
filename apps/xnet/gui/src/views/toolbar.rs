//! Toolbar view with network control buttons.

use gpui::{div, prelude::*, px};

use crate::{state::NetworkStatus, ui};

/// Renders the toolbar with network control buttons.
///
/// The `busy` parameter disables all buttons when an operation is in progress.
/// Returns a container with clickable button elements.
pub fn render_toolbar(status: NetworkStatus, busy: bool) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .w_full()
        .gap(px(10.0))
        .px(px(20.0))
        .py(px(10.0))
        .child(ui::success_button("Up", busy))
        .child(ui::warning_button("Down", busy))
        .child(ui::danger_button("Delete", busy))
        .child(ui::primary_button(
            "Add Node",
            busy || status != NetworkStatus::Running,
        ))
}
