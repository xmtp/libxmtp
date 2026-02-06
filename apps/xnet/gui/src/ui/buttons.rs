//! Button helper functions for common button patterns.

use gpui::{Div, Hsla, SharedString, div, prelude::*, px};

use crate::theme;

/// Creates a styled button with the given label and background color.
///
/// # Example
/// ```rust
/// make_button("Click me", theme::btn_primary(), false)
/// ```
pub fn make_button(label: impl Into<SharedString>, bg: Hsla, disabled: bool) -> impl IntoElement {
    div()
        .px(px(16.0))
        .py(px(6.0))
        .bg(bg)
        .rounded(px(6.0))
        .when(!disabled, |this: Div| this.cursor_pointer())
        .text_color(theme::btn_text())
        .text_sm()
        .child(label.into())
}

/// Creates a primary-styled button.
pub fn primary_button(label: impl Into<SharedString>, disabled: bool) -> impl IntoElement {
    let bg = if disabled {
        theme::text_muted()
    } else {
        theme::btn_primary()
    };
    make_button(label, bg, disabled)
}

/// Creates a danger-styled button (red).
pub fn danger_button(label: impl Into<SharedString>, disabled: bool) -> impl IntoElement {
    let bg = if disabled {
        theme::text_muted()
    } else {
        theme::btn_danger()
    };
    make_button(label, bg, disabled)
}

/// Creates a warning-styled button (yellow).
pub fn warning_button(label: impl Into<SharedString>, disabled: bool) -> impl IntoElement {
    let bg = if disabled {
        theme::text_muted()
    } else {
        theme::btn_warning()
    };
    make_button(label, bg, disabled)
}

/// Creates a success-styled button (green).
pub fn success_button(label: impl Into<SharedString>, disabled: bool) -> impl IntoElement {
    let bg = if disabled {
        theme::text_muted()
    } else {
        theme::btn_success()
    };
    make_button(label, bg, disabled)
}
