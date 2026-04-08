//! Header view with application title and status badge.

use std::sync::Arc;

use gpui::{Hsla, Image, ImageFormat, SharedString, div, img, prelude::*, px};

use crate::{state::NetworkStatus, theme, ui};

static LOGO_BYTES: &[u8] = include_bytes!("../../assets/logo.png");

/// Renders the application header with title and status indicator.
pub fn render_header(status: NetworkStatus) -> impl IntoElement {
    let (status_text, badge_color): (SharedString, Hsla) = match status {
        NetworkStatus::Stopped => ("Stopped".into(), theme::text_muted()),
        NetworkStatus::Starting => ("Starting…".into(), theme::accent_yellow()),
        NetworkStatus::Running => ("Running".into(), theme::accent_green()),
        NetworkStatus::Stopping => ("Stopping…".into(), theme::accent_yellow()),
        NetworkStatus::Deleting => ("Deleting…".into(), theme::accent_yellow()),
        NetworkStatus::Error => ("Error".into(), theme::accent_red()),
    };

    div()
        .flex()
        .flex_row()
        .w_full()
        .justify_between()
        .items_center()
        .px(px(20.0))
        .py(px(12.0))
        .bg(theme::bg_surface())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(
                    img(Arc::new(Image::from_bytes(
                        ImageFormat::Png,
                        LOGO_BYTES.to_vec(),
                    )))
                    .size(px(32.0)),
                )
                .child(
                    div()
                        .text_color(theme::text_primary())
                        .text_xl()
                        .child("xnet"),
                )
                .child(
                    div()
                        .text_color(theme::text_muted())
                        .text_xs()
                        .child(SharedString::from(xnet::get_version())),
                ),
        )
        .child(ui::status_badge(badge_color, status_text))
}
