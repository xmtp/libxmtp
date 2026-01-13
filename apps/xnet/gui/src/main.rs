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

use gpui::{Application, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
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
        let bounds = Bounds::centered(None, size(px(1_000.0), px(640.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx: &mut gpui::App| {
                cx.new(|cx| {
                    let mut view = RootView::new();
                    view.start_log_drain(log_rx, cx);
                    view.run_docker_check(cx);
                    view
                })
            },
        )
        .expect("failed to open window");
    });
}
