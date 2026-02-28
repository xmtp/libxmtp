//! xnet-gui — GPUI frontend for the XMTP Network Testing Framework.
//!
//! Provides a graphical interface for managing Docker services (Up / Down / Delete)
//! and for adding XMTPD nodes, with live info panels showing running services.
//!
//! ## Architecture
//!
//! This application follows a module-based architecture:
//!
//! - `state/` - Application state models
//! - `theme/` - Color palette and spacing constants
//! - `ui/` - Reusable UI helper functions (buttons, panels, badges)
//! - `views/` - View rendering modules
//! - `actions/` - Business logic for async operations
//! - `prelude/` - Common imports
//!
//! See README.md for detailed architecture documentation.

mod actions;
mod prelude;
mod state;
pub mod theme;
pub mod ui;
mod views;

use std::sync::Arc;

use gpui::{Application, Bounds, WindowBounds, WindowOptions, prelude::*, px, rgb, size};
use gpui_tokio_bridge::init;
use tokio::sync::mpsc;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};
use views::root::RootView;

/// A custom tracing [`Layer`] that sends formatted log events over an unbounded
/// channel so the GUI can display them in the log panel.
struct ChannelLayer {
    tx: mpsc::UnboundedSender<Arc<str>>,
}

impl<S: tracing::Subscriber> Layer<S> for ChannelLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        let target = metadata.target();

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let line = format!(
            "[{}] {}: {}",
            metadata.level(),
            target,
            visitor.message.unwrap_or_default()
        );
        let _ = self.tx.send(Arc::from(line));
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }
}

fn main() {
    // Initialise colour-eyre so any panics render nicely in stderr even when
    // running as a GUI application.
    color_eyre::install().ok();

    let (log_tx, log_rx) = mpsc::unbounded_channel();

    let filter = EnvFilter::builder().with_env_var("XNET_LOG").try_from_env();
    let filter = if let Ok(filter) = filter {
        filter
    } else {
        EnvFilter::builder()
            .parse("xnet_lib=debug,xnet_gui=debug")
            .unwrap()
    };
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .with(ChannelLayer { tx: log_tx })
        .init();

    Application::new().run(|cx: &mut gpui::App| {
        init(cx);
        gpui_component::init(cx);

        // Override gpui-component's default theme with Catppuccin Mocha colors.
        // This ensures Root::render() applies our palette and Button variants
        // (primary, danger, etc.) use the correct colors.
        {
            let theme = gpui_component::theme::Theme::global_mut(cx);
            theme.background = rgb(0x1E1E2E).into();
            theme.foreground = rgb(0xCDD6F4).into();
            theme.primary = rgb(0x89B4FA).into();
            theme.primary_foreground = rgb(0x1E1E2E).into();
            theme.primary_hover = rgb(0xB4D0FB).into();
            theme.primary_active = rgb(0x74A8F8).into();
            theme.danger = rgb(0xF38BA8).into();
            theme.danger_foreground = rgb(0x1E1E2E).into();
            theme.danger_hover = rgb(0xF5A0B8).into();
            theme.danger_active = rgb(0xF07090).into();
            theme.success = rgb(0xA6E3A1).into();
            theme.success_foreground = rgb(0x1E1E2E).into();
            theme.success_hover = rgb(0xBDEBB9).into();
            theme.success_active = rgb(0x8ED888).into();
            theme.warning = rgb(0xF9E2AF).into();
            theme.warning_foreground = rgb(0x1E1E2E).into();
            theme.warning_hover = rgb(0xFBECC8).into();
            theme.warning_active = rgb(0xF5D68A).into();
            theme.info = rgb(0x89B4FA).into();
            theme.info_foreground = rgb(0x1E1E2E).into();
            theme.info_hover = rgb(0xB4D0FB).into();
            theme.info_active = rgb(0x74A8F8).into();
            theme.secondary = rgb(0x2A2A3C).into();
            theme.secondary_foreground = rgb(0xA6ADC8).into();
            theme.secondary_hover = rgb(0x353548).into();
            theme.secondary_active = rgb(0x404058).into();
            theme.muted = rgb(0x6C7086).into();
            theme.muted_foreground = rgb(0x6C7086).into();
            theme.border = rgb(0x353548).into();
            theme.input = rgb(0x353548).into();
            theme.ring = rgb(0x89B4FA).into();
        }

        let bounds = Bounds::centered(None, size(px(1_000.0), px(640.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx: &mut gpui::App| {
                let view = cx.new(|cx| {
                    let mut view = RootView::new(window, cx);
                    view.start_log_drain(log_rx, cx);
                    view.run_docker_check(cx);
                    view
                });
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            },
        )
        .expect("failed to open window");
    });
}
