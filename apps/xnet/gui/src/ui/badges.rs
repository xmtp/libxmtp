//! Status badge components.

use gpui::{Hsla, SharedString, div, prelude::*, px};

use crate::theme;

/// Creates a status badge with a colored dot and text.
///
/// # Example
/// ```rust
/// status_badge(theme::accent_green(), "Running")
/// ```
pub fn status_badge(color: Hsla, text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .child(div().size(px(10.0)).rounded(px(5.0)).bg(color))
        .child(
            div()
                .text_color(theme::text_secondary())
                .text_sm()
                .child(text.into()),
        )
}

/// Creates a small status indicator dot (used in list items).
pub fn status_dot(color: Hsla) -> impl IntoElement {
    div().size(px(8.0)).rounded(px(4.0)).bg(color)
}
