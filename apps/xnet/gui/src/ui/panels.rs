//! Panel container helpers.

use gpui::{Div, Hsla, SharedString, div, prelude::*, px};

use crate::theme;

/// Creates a panel container with consistent styling.
///
/// Returns a div that can be further customized with `.child()` calls.
///
/// # Example
/// ```rust
/// panel_container()
///     .w(px(300.0))
///     .child(panel_title("Services"))
///     .child(div().child("Content"))
/// ```
pub fn panel_container() -> Div {
    div()
        .flex()
        .flex_col()
        .bg(theme::bg_surface())
        .rounded(px(8.0))
        .p(px(12.0))
        .gap(px(6.0))
        .overflow_hidden()
}

/// Creates a panel title with accent color.
pub fn panel_title(text: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    div().text_color(color).text_sm().child(text.into())
}

/// Creates an empty state message for panels.
pub fn empty_state(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_color(theme::text_muted())
        .text_xs()
        .child(text.into())
}

/// Creates a list item row with a status dot and text.
pub fn list_item_row(dot_color: Hsla, content: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .py(px(2.0))
        .child(div().size(px(8.0)).rounded(px(4.0)).bg(dot_color))
        .child(content)
}
