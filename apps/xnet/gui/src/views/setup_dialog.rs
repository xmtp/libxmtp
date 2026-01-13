//! Setup dialog — blocks interaction until Docker and DNS checks pass.

use gpui::{ClipboardItem, SharedString, div, prelude::*, px};

use crate::{
    state::{CheckStatus, SetupChecks, SetupPhase},
    theme,
};

/// Spinner frames: a signal propagating across connected nodes.
const SPINNER_FRAMES: &[&str] = &["○─○─○", "●─○─○", "○─●─○", "○─○─●"];

/// Renders the dialog card for the setup overlay.
///
/// The backdrop and occlusion are handled by the caller (`RootView::render`);
/// this function only produces the centered card content.
pub fn render_setup_dialog(checks: &SetupChecks) -> impl IntoElement {
    let mut card = div()
        .flex()
        .flex_col()
        .bg(theme::bg_surface())
        .rounded(px(12.0))
        .p(px(24.0))
        .gap(px(16.0))
        .w(px(520.0));

    // Title
    card = card.child(
        div()
            .text_color(theme::text_primary())
            .text_xl()
            .child("System Requirements"),
    );

    // Docker check row (always shown)
    card = card.child(render_check_row(
        "Docker",
        checks.docker,
        checks.docker_error.as_deref(),
        checks.spinner_tick,
    ));

    // DNS check row (only shown in DNS phase)
    if checks.phase == SetupPhase::Dns {
        card = card.child(render_check_row(
            "DNS (*.xmtpd.local)",
            checks.dns,
            checks.dns_error.as_deref(),
            checks.spinner_tick,
        ));
    }

    // Platform-specific instructions for failures
    if checks.docker == CheckStatus::Failed {
        card = card.child(render_docker_instructions());
    }
    if checks.phase == SetupPhase::Dns && checks.dns == CheckStatus::Failed {
        card = card.child(render_dns_instructions());
    }

    card
}

fn render_check_row(
    label: &str,
    status: CheckStatus,
    error: Option<&str>,
    spinner_tick: usize,
) -> impl IntoElement {
    let (icon, color) = match status {
        CheckStatus::Pending => ("?", theme::text_muted()),
        CheckStatus::Checking => (
            SPINNER_FRAMES[spinner_tick % SPINNER_FRAMES.len()],
            theme::accent_yellow(),
        ),
        CheckStatus::Passed => ("OK", theme::accent_green()),
        CheckStatus::Failed => ("FAIL", theme::accent_red()),
    };

    let mut col = div().flex().flex_col().gap(px(4.0)).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_color(color)
                    .text_sm()
                    .w(px(64.0))
                    .child(SharedString::from(icon)),
            )
            .child(
                div()
                    .text_color(theme::text_primary())
                    .text_sm()
                    .child(SharedString::from(label.to_owned())),
            ),
    );

    if let Some(err) = error {
        col = col.child(
            div()
                .pl(px(44.0))
                .text_color(theme::accent_red())
                .text_xs()
                .child(SharedString::from(err.to_owned())),
        );
    }

    col
}

fn render_docker_instructions() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .bg(theme::bg_primary())
        .rounded(px(8.0))
        .p(px(12.0))
        .child(
            div()
                .text_color(theme::accent_yellow())
                .text_sm()
                .child("Docker Setup"),
        )
        .child(
            div()
                .text_color(theme::text_secondary())
                .text_xs()
                .child("Docker must be installed and the daemon must be running."),
        )
        .child(render_platform_docker_hint())
}

fn render_platform_docker_hint() -> impl IntoElement {
    #[cfg(target_os = "macos")]
    {
        div()
            .text_color(theme::text_secondary())
            .text_xs()
            .child("Start Docker Desktop from Applications.")
    }
    #[cfg(target_os = "linux")]
    {
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_color(theme::text_secondary())
                    .text_xs()
                    .child("Start the Docker daemon:"),
            )
            .child(code_block("sudo systemctl start docker"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        div()
            .text_color(theme::text_secondary())
            .text_xs()
            .child("Ensure Docker is installed and the daemon is running.")
    }
}

fn render_dns_instructions() -> impl IntoElement {
    let mut instructions = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .bg(theme::bg_primary())
        .rounded(px(8.0))
        .p(px(12.0))
        .child(
            div()
                .text_color(theme::accent_yellow())
                .text_sm()
                .child("DNS Setup Instructions"),
        );

    instructions = render_platform_dns_instructions(instructions);

    instructions
}

#[cfg(target_os = "macos")]
fn render_platform_dns_instructions(container: gpui::Div) -> gpui::Div {
    container
        .child(
            div()
                .text_color(theme::text_secondary())
                .text_xs()
                .child("Create a resolver configuration:"),
        )
        .child(code_block("sudo mkdir -p /etc/resolver"))
        .child(code_block(
            "sudo tee /etc/resolver/xmtpd.local <<EOF\nnameserver 127.0.0.1\nport 5354\nEOF",
        ))
        .child(
            div()
                .text_color(theme::text_muted())
                .text_xs()
                .child("Verify: scutil --dns | grep xmtpd.local"),
        )
}

#[cfg(target_os = "linux")]
fn render_platform_dns_instructions(container: gpui::Div) -> gpui::Div {
    container
        .child(
            div()
                .text_color(theme::text_secondary())
                .text_xs()
                .child("For systemd-resolved, run:"),
        )
        .child(code_block("sudo mkdir -p /etc/systemd/resolved.conf.d"))
        .child(code_block(
            "sudo tee /etc/systemd/resolved.conf.d/xmtp.conf <<EOF\n[Resolve]\nDNS=127.0.0.1:5354\nDomains=~xmtpd.local\nEOF",
        ))
        .child(code_block("sudo systemctl restart systemd-resolved"))
        .child(
            div()
                .text_color(theme::text_muted())
                .text_xs()
                .child("Verify: resolvectl status"),
        )
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn render_platform_dns_instructions(container: gpui::Div) -> gpui::Div {
    container.child(
        div().text_color(theme::text_secondary()).text_xs().child(
            "Configure your system DNS to resolve *.xmtpd.local to 127.0.0.1 via port 5354.",
        ),
    )
}

fn code_block(text: &str) -> impl IntoElement {
    let text_owned: SharedString = text.to_owned().into();
    let text_for_clipboard = text.to_owned();

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .child(
            div()
                .flex_grow()
                .bg(gpui::rgb(0x181825))
                .rounded(px(4.0))
                .px(px(8.0))
                .py(px(4.0))
                .text_color(theme::accent_green())
                .text_xs()
                .child(text_owned),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "copy-{}",
                    &text_for_clipboard[..text_for_clipboard.len().min(32)]
                )))
                .cursor_pointer()
                .px(px(6.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .bg(gpui::rgb(0x181825))
                .hover(|s| s.bg(theme::bg_surface()))
                .text_color(theme::text_muted())
                .text_xs()
                .child("copy")
                .on_click(move |_, _window, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(text_for_clipboard.clone()));
                }),
        )
}
