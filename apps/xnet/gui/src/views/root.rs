//! Root view — the top-level window content.
//!
//! This module orchestrates the main application layout and delegates rendering
//! to specialized view modules. It handles action coordination and state updates.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Local, TimeZone, Utc};
use futures::StreamExt;
use gpui::{
    AsyncApp, ClickEvent, Context, Entity, ScrollHandle, SharedString, Timer, WeakEntity, Window,
    deferred, div, hsla, prelude::*, px, rgb,
};
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariant, ButtonVariants};
use gpui_component::input::InputState;
use gpui_component::{Disableable, Sizable};
use gpui_tokio_bridge::Tokio;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{error, info, warn};

use crate::{
    actions,
    state::{AppState, CheckStatus, NetworkStatus, Page, SetupChecks, SetupPhase, ToxicsState},
    theme, ui, views,
};
use color_eyre::eyre::{Error, Result};

// ---------------------------------------------------------------------------
// RootView
// ---------------------------------------------------------------------------

pub struct RootView {
    state: AppState,
    /// Whether a background operation is in-flight (disables buttons).
    busy: bool,
    /// Scroll handle for the log panel so we can auto-scroll to bottom.
    log_scroll: ScrollHandle,
    /// Startup dependency check state.
    setup: SetupChecks,
    /// Toxics management page state.
    toxics: ToxicsState,
    /// Text input for custom migration offset (e.g. "2h30m").
    migrate_input: Entity<InputState>,
    /// Text input for custom latency value (in ms) on the Toxics page.
    latency_input: Entity<InputState>,
    /// Abort handles for background poller tasks — aborted on down/delete.
    poller_handles: Vec<AbortHandle>,
}

impl RootView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let migrate_input = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. 2h30m"));
        let latency_input = cx.new(|cx| InputState::new(window, cx).placeholder("ms"));
        Self {
            state: AppState::new(),
            busy: false,
            log_scroll: ScrollHandle::new(),
            setup: SetupChecks::new(),
            toxics: ToxicsState::new(),
            migrate_input,
            latency_input,
            poller_handles: Vec::new(),
        }
    }

    // -- Log Drain ------------------------------------------------------------

    /// Drains tracing log events from the channel into the GUI log panel.
    pub fn start_log_drain(
        &mut self,
        rx: mpsc::UnboundedReceiver<Arc<str>>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut stream = UnboundedReceiverStream::new(rx);
            while let Some(line) = stream.next().await {
                let _ = cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.state.push_log(line);
                        view.log_scroll.scroll_to_bottom();
                        cx.notify();
                    });
                });
            }
            Ok::<_, Error>(())
        })
        .detach();
    }

    // -- Setup Checks ---------------------------------------------------------

    /// Runs the Docker availability check at startup.
    pub fn run_docker_check(&mut self, cx: &mut Context<Self>) {
        self.setup.phase = SetupPhase::Docker;
        self.setup.docker = CheckStatus::Checking;
        self.setup.rechecking = true;
        self.setup.spinner_tick = 0;
        self.start_spinner(cx);
        cx.notify();

        let check = Tokio::spawn(cx, actions::check_docker());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = check.await?;
            cx.update(|cx| {
                this.update(cx, |view, cx| {
                    match result {
                        Ok(()) => {
                            view.setup.docker = CheckStatus::Passed;
                            view.setup.docker_error = None;
                            view.setup.phase = SetupPhase::Ready;
                            info!("Docker check passed.");
                        }
                        Err(e) => {
                            view.setup.docker = CheckStatus::Failed;
                            view.setup.docker_error = Some(e.to_string());
                            warn!("Docker check failed: {}", e);
                        }
                    }
                    view.setup.rechecking = false;
                    cx.notify();
                    Ok::<_, Error>(())
                })
            })
        })
        .detach();
    }

    /// Runs the DNS resolution check (called after "Up" succeeds).
    fn run_dns_check(&mut self, cx: &mut Context<Self>) {
        self.setup.phase = SetupPhase::Dns;
        self.setup.dns = CheckStatus::Checking;
        self.setup.rechecking = true;
        self.setup.spinner_tick = 0;
        self.start_spinner(cx);
        cx.notify();

        let check = Tokio::spawn(cx, actions::check_dns());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = check.await?;
            cx.update(|cx| {
                this.update(cx, |view, cx| {
                    match result {
                        Ok(()) => {
                            view.setup.dns = CheckStatus::Passed;
                            view.setup.dns_error = None;
                            view.setup.phase = SetupPhase::Ready;
                            info!("DNS check passed.");
                        }
                        Err(e) => {
                            view.setup.dns = CheckStatus::Failed;
                            view.setup.dns_error = Some(e.to_string());
                            warn!("DNS check failed: {}", e);
                        }
                    }
                    view.setup.rechecking = false;
                    cx.notify();
                    Ok::<_, Error>(())
                })
            })
        })
        .detach();
    }

    /// Re-runs the check for the current setup phase.
    fn action_recheck(&mut self, cx: &mut Context<Self>) {
        if self.setup.rechecking {
            return;
        }
        match self.setup.phase {
            SetupPhase::Docker => self.run_docker_check(cx),
            SetupPhase::Dns => self.run_dns_check(cx),
            SetupPhase::Ready => {}
        }
    }

    /// Starts a repeating timer that advances the spinner animation frame.
    fn start_spinner(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                Timer::after(Duration::from_millis(200)).await;
                let should_continue = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        if !view.setup.rechecking {
                            return false;
                        }
                        view.setup.spinner_tick = view.setup.spinner_tick.wrapping_add(1);
                        cx.notify();
                        true
                    })
                });
                match should_continue {
                    Ok(Ok(true)) => {}
                    _ => break,
                }
            }
            Ok::<_, Error>(())
        })
        .detach();
    }

    // -- Poller Lifecycle -----------------------------------------------------

    /// Abort all running poller tasks (service, node, cutover, toxics).
    fn stop_pollers(&mut self) {
        for handle in self.poller_handles.drain(..) {
            handle.abort();
        }
    }

    // -- Action Handlers -----------------------------------------------------

    fn action_up(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.state.network_status = NetworkStatus::Starting;
        info!("Starting services…");
        cx.notify();

        let result = Tokio::spawn(cx, actions::execute_up());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = result.await?;
            cx.update(|cx| {
                this.update(cx, |view: &mut RootView, cx: &mut Context<RootView>| {
                    view.busy = false;
                    match result {
                        Ok(()) => {
                            view.state.network_status = NetworkStatus::Running;
                            info!("Services started successfully.");
                            // Start polling for services and nodes
                            view.start_service_poller(cx);
                            view.start_node_poller(cx);
                            view.start_cutover_poller(cx);
                            // Check DNS now that CoreDNS is running
                            view.run_dns_check(cx);
                        }
                        Err(msg) => {
                            view.state.network_status = NetworkStatus::Error;
                            let msg = msg.to_string();
                            view.state.last_error = Some(msg.clone());
                            error!("{}", msg);
                        }
                    }
                    cx.notify();
                    Ok::<_, Error>(())
                })
            })
        })
        .detach();
    }

    fn action_down(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.stop_pollers();
        self.busy = true;
        self.state.network_status = NetworkStatus::Stopping;
        info!("Stopping services…");
        cx.notify();

        let result = Tokio::spawn(cx, actions::execute_down());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = result.await?;
            cx.update(|cx| {
                let _ = this.update(cx, |view: &mut RootView, cx: &mut Context<RootView>| {
                    view.busy = false;
                    match result {
                        Ok(()) => {
                            view.state.network_status = NetworkStatus::Stopped;
                            view.state.services.clear();
                            view.state.xmtpd_nodes.clear();
                            view.state.cutover_ns = None;
                            info!("Services stopped.");
                        }
                        Err(msg) => {
                            view.state.network_status = NetworkStatus::Error;
                            let msg = msg.to_string();
                            view.state.last_error = Some(msg.clone());
                            error!("{}", msg);
                        }
                    }
                    cx.notify();
                });
            })
        })
        .detach();
    }

    fn action_delete(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.stop_pollers();
        self.busy = true;
        self.state.network_status = NetworkStatus::Deleting;
        info!("Deleting all resources…");
        cx.notify();

        let result = Tokio::spawn(cx, actions::execute_delete());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result: Result<()> = result.await?;

            cx.update(|cx| {
                let _ = this.update(cx, |view: &mut RootView, cx: &mut Context<RootView>| {
                    view.busy = false;
                    match result {
                        Ok(()) => {
                            view.state.network_status = NetworkStatus::Stopped;
                            view.state.services.clear();
                            view.state.xmtpd_nodes.clear();
                            view.state.cutover_ns = None;
                            view.state.last_error = None;
                            info!("All resources deleted.");
                        }
                        Err(msg) => {
                            view.state.network_status = NetworkStatus::Error;
                            let msg = msg.to_string();
                            view.state.last_error = Some(msg.clone());
                            error!("{}", msg);
                        }
                    }
                    cx.notify();
                });
            })
        })
        .detach();
    }

    fn action_add_node(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if !actions::can_add_node(self.state.network_status) {
            warn!("Cannot add node: services are not running.");
            cx.notify();
            return;
        }
        self.busy = true;
        info!("Registering new XMTPD node…");
        cx.notify();

        let result = Tokio::spawn(cx, actions::execute_add_node());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = result.await?;

            cx.update(|cx| {
                let _ = this.update(cx, |view: &mut RootView, cx: &mut Context<RootView>| {
                    view.busy = false;
                    match result {
                        Ok(node_info) => {
                            info!("Node {} registered.", node_info.id);
                        }
                        Err(msg) => {
                            let msg = msg.to_string();
                            view.state.last_error = Some(msg.clone());
                            error!("{}", msg);
                        }
                    }
                    cx.notify();
                    Ok::<_, Error>(())
                });
            })
        })
        .detach();
    }

    fn action_add_migrator(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if !actions::can_add_node(self.state.network_status) {
            warn!("Cannot add migrator: services are not running.");
            cx.notify();
            return;
        }
        self.busy = true;
        info!("Registering new XMTPD migrator…");
        cx.notify();

        let result = Tokio::spawn(cx, actions::execute_add_migrator());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = result.await?;

            cx.update(|cx| {
                let _ = this.update(cx, |view: &mut RootView, cx: &mut Context<RootView>| {
                    view.busy = false;
                    match result {
                        Ok(node_info) => {
                            info!("Migrator {} registered.", node_info.id);
                        }
                        Err(msg) => {
                            let msg = msg.to_string();
                            view.state.last_error = Some(msg.clone());
                            error!("{}", msg);
                        }
                    }
                    cx.notify();
                    Ok::<_, Error>(())
                });
            })
        })
        .detach();
    }

    // -- Migration Action -----------------------------------------------------

    fn action_migrate(&mut self, cutover_offset: Option<String>, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let label = cutover_offset.as_deref().unwrap_or("now").to_string();
        info!("Setting d14n cutover to {}…", label);
        cx.notify();

        let result = Tokio::spawn(cx, actions::execute_migrate(cutover_offset));

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = result.await?;
            cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.busy = false;
                    match result {
                        Ok(()) => info!("Migration cutover set."),
                        Err(e) => {
                            let msg = e.to_string();
                            error!("{}", msg);
                            view.state.last_error = Some(msg);
                        }
                    }
                    cx.notify();
                    Ok::<_, Error>(())
                })
            })
        })
        .detach();
    }

    // -- Node Poller ----------------------------------------------------------

    /// Spawns a background task that polls GetNodes and streams updates to the UI.
    fn start_node_poller(&mut self, cx: &mut Context<Self>) {
        let poller = Tokio::spawn(cx, actions::start_node_poller());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let (mut rx, abort_handle) = match poller.await? {
                Ok(pair) => pair,
                Err(e) => {
                    error!("Failed to start node poller: {e}");
                    return Ok(());
                }
            };
            let _ = cx.update(|cx| {
                let _ = this.update(cx, |view, _cx| {
                    view.poller_handles.push(abort_handle);
                });
            });
            while let Some(nodes) = rx.recv().await {
                let should_stop = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        if view.state.network_status != NetworkStatus::Running {
                            return Ok::<_, Error>(true);
                        }
                        view.state.xmtpd_nodes = nodes;
                        cx.notify();
                        Ok(false)
                    })
                });
                if should_stop
                    .ok()
                    .and_then(|r| r.ok())
                    .and_then(|r| r.ok())
                    .unwrap_or(true)
                {
                    break;
                }
            }
            Ok::<_, Error>(())
        })
        .detach();
    }

    // -- Service Poller -------------------------------------------------------

    /// Spawns a background task that polls ToxiProxy and streams service updates to the UI.
    fn start_service_poller(&mut self, cx: &mut Context<Self>) {
        let poller = Tokio::spawn(cx, actions::start_service_poller());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let (mut rx, abort_handle) = match poller.await? {
                Ok(pair) => pair,
                Err(e) => {
                    error!("Failed to start service poller: {e}");
                    return Ok(());
                }
            };
            let _ = cx.update(|cx| {
                let _ = this.update(cx, |view, _cx| {
                    view.poller_handles.push(abort_handle);
                });
            });
            while let Some(services) = rx.recv().await {
                let should_stop = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        if view.state.network_status != NetworkStatus::Running {
                            return Ok::<_, Error>(true);
                        }
                        view.state.services = services;
                        cx.notify();
                        Ok(false)
                    })
                });
                if should_stop
                    .ok()
                    .and_then(|r| r.ok())
                    .and_then(|r| r.ok())
                    .unwrap_or(true)
                {
                    break;
                }
            }
            Ok::<_, Error>(())
        })
        .detach();
    }

    // -- Cutover Poller -------------------------------------------------------

    /// Spawns a background task that polls FetchD14nCutover and streams updates to the UI.
    fn start_cutover_poller(&mut self, cx: &mut Context<Self>) {
        let poller = Tokio::spawn(cx, actions::start_cutover_poller());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let (mut rx, abort_handle) = match poller.await? {
                Ok(pair) => pair,
                Err(e) => {
                    error!("Failed to start cutover poller: {e}");
                    return Ok(());
                }
            };
            let _ = cx.update(|cx| {
                let _ = this.update(cx, |view, _cx| {
                    view.poller_handles.push(abort_handle);
                });
            });
            while let Some(cutover_ns) = rx.recv().await {
                let should_stop = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        if view.state.network_status != NetworkStatus::Running {
                            return Ok::<_, Error>(true);
                        }
                        view.state.cutover_ns = Some(cutover_ns);
                        cx.notify();
                        Ok(false)
                    })
                });
                if should_stop
                    .ok()
                    .and_then(|r| r.ok())
                    .and_then(|r| r.ok())
                    .unwrap_or(true)
                {
                    break;
                }
            }
            Ok::<_, Error>(())
        })
        .detach();
    }

    // -- Toolbar Rendering with Click Handlers -------------------------------

    /// Renders the toolbar with clickable buttons.
    ///
    /// We need to attach click handlers here since they require `cx.listener()`.
    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.busy;
        let can_add_node = !disabled && self.state.network_status == NetworkStatus::Running;

        let mauve_variant = ButtonVariant::Custom(
            ButtonCustomVariant::new(cx)
                .color(theme::accent_mauve())
                .foreground(theme::btn_text())
                .hover(rgb(0xD4B8F9).into())
                .active(rgb(0xBE94F5).into()),
        );

        div()
            .flex()
            .flex_row()
            .w_full()
            .gap(px(10.0))
            .px(px(20.0))
            .py(px(10.0))
            .child(self.make_clickable_button(
                "btn-up",
                "Up",
                ButtonVariant::Success,
                disabled,
                cx,
                |view, _, _, cx| view.action_up(cx),
            ))
            .child(self.make_clickable_button(
                "btn-down",
                "Down",
                ButtonVariant::Warning,
                disabled,
                cx,
                |view, _, _, cx| view.action_down(cx),
            ))
            .child(self.make_clickable_button(
                "btn-delete",
                "Delete",
                ButtonVariant::Danger,
                disabled,
                cx,
                |view, _, _, cx| view.action_delete(cx),
            ))
            .child(self.make_clickable_button(
                "btn-add-node",
                "Add Node",
                ButtonVariant::Primary,
                !can_add_node,
                cx,
                |view, _, _, cx| view.action_add_node(cx),
            ))
            .child(self.make_clickable_button(
                "btn-add-migrator",
                "Add XMTPD Migrator",
                ButtonVariant::Primary,
                !can_add_node,
                cx,
                |view, _, _, cx| view.action_add_migrator(cx),
            ))
            .child(self.make_clickable_button(
                "btn-toxics",
                "Toxics",
                mauve_variant,
                !can_add_node,
                cx,
                |view, _, _, cx| view.action_navigate_toxics(cx),
            ))
    }

    /// Helper to create a clickable button with event handler.
    fn make_clickable_button(
        &self,
        id: &'static str,
        label: &'static str,
        variant: ButtonVariant,
        disabled: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        Button::new(id)
            .label(label)
            .small()
            .with_variant(variant)
            .disabled(disabled)
            .on_click(cx.listener(move |view, event, window, cx| {
                on_click(view, event, window, cx);
            }))
    }

    /// Renders the two-panel layout (services and nodes).
    fn render_panels(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .w_full()
            .flex_grow()
            .gap(px(12.0))
            .px(px(20.0))
            .py(px(8.0))
            .overflow_hidden()
            .child(views::panels::render_services_panel(&self.state.services))
            .child(views::panels::render_nodes_panel(&self.state.xmtpd_nodes))
    }

    // -- Cutover Section Rendering --------------------------------------------

    /// Renders the D14N cutover display and migration preset buttons.
    fn render_cutover_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let can_migrate = !self.busy && self.state.network_status == NetworkStatus::Running;

        let mut container = ui::panel_container()
            .w_full()
            .child(ui::panel_title("D14N Cutover", theme::accent_yellow()));

        // -- Cutover time display --
        match self.state.cutover_ns {
            Some(ts_ns) => {
                let secs = (ts_ns / 1_000_000_000) as i64;
                let nanos = (ts_ns % 1_000_000_000) as u32;

                let local_str: SharedString = Local
                    .timestamp_opt(secs, nanos)
                    .single()
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S %Z").to_string())
                    .unwrap_or_else(|| "invalid timestamp".to_string())
                    .into();

                let utc_str: SharedString = Utc
                    .timestamp_opt(secs, nanos)
                    .single()
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| "invalid timestamp".to_string())
                    .into();

                let ns_str: SharedString = format!("{} ns", ts_ns).into();

                container = container
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_color(theme::text_primary())
                                    .text_xs()
                                    .child(local_str),
                            )
                            .child(
                                div()
                                    .text_color(theme::text_secondary())
                                    .text_xs()
                                    .child(utc_str),
                            ),
                    )
                    .child(
                        div()
                            .text_color(theme::text_muted())
                            .text_xs()
                            .child(ns_str),
                    );
            }
            None => {
                container = container.child(ui::empty_state("Cutover time not available"));
            }
        }

        // -- Migration preset buttons + custom input --
        let presets: &[(&str, Option<&str>)] = &[
            ("Now", None),
            ("30s", Some("30s")),
            ("5m", Some("5m")),
            ("1h", Some("1h")),
        ];

        let mut button_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .pt(px(4.0))
            .child(
                div()
                    .text_color(theme::text_secondary())
                    .text_xs()
                    .child("Set cutover:"),
            );

        for &(label, offset) in presets {
            let offset_owned: Option<String> = offset.map(|s| s.to_string());
            let id: SharedString = format!("btn-migrate-{}", label.to_lowercase()).into();
            button_row = button_row.child(self.make_dynamic_button(
                id,
                label,
                ButtonVariant::Warning,
                !can_migrate,
                cx,
                move |view, _, _, cx| {
                    view.action_migrate(offset_owned.clone(), cx);
                },
            ));
        }

        // Custom offset input
        let input = gpui_component::input::Input::new(&self.migrate_input)
            .small()
            .w(px(120.0));
        button_row = button_row.child(input);

        // "Set" button that reads the custom input value
        button_row = button_row.child(self.make_dynamic_button(
            SharedString::from("btn-migrate-custom"),
            "Set",
            ButtonVariant::Primary,
            !can_migrate,
            cx,
            |view, _, _, cx| {
                let value = view.migrate_input.read(cx).value().to_string();
                if !value.is_empty() {
                    view.action_migrate(Some(value), cx);
                }
            },
        ));

        container.child(button_row)
    }

    // -- Dynamic Button Helper ------------------------------------------------

    /// Like `make_clickable_button` but accepts dynamic (non-static) id and label.
    fn make_dynamic_button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<gpui::SharedString>,
        variant: ButtonVariant,
        disabled: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        Button::new(id)
            .label(label)
            .xsmall()
            .with_variant(variant)
            .disabled(disabled)
            .on_click(cx.listener(move |view, event, window, cx| {
                on_click(view, event, window, cx);
            }))
    }

    // -- Page Navigation ------------------------------------------------------

    fn action_navigate_toxics(&mut self, cx: &mut Context<Self>) {
        self.state.page = Page::Toxics;
        self.start_toxics_poller(cx);
        cx.notify();
    }

    fn action_navigate_dashboard(&mut self, cx: &mut Context<Self>) {
        self.state.page = Page::Dashboard;
        cx.notify();
    }

    // -- Toxics Page Rendering ------------------------------------------------

    fn render_toxics_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .px(px(20.0))
            .py(px(10.0))
            .child(self.make_clickable_button(
                "btn-back",
                "< Dashboard",
                ButtonVariant::Primary,
                false,
                cx,
                |view, _, _, cx| view.action_navigate_dashboard(cx),
            ))
            .child(self.make_clickable_button(
                "btn-reset-all",
                "Reset All Toxics",
                ButtonVariant::Danger,
                self.busy,
                cx,
                |view, _, _, cx| view.action_reset_all_toxics(cx),
            ))
    }

    /// Proxy names eligible for latency injection (infrastructure services).
    const INFRA_PROXIES: &[&str] = &["anvil", "gateway", "node-go"];

    fn render_toxics_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let xmtpd_names: Vec<String> = self
            .state
            .services
            .iter()
            .filter(|s| s.name.starts_with("xmtpd"))
            .map(|s| s.name.clone())
            .collect();

        let infra_names: Vec<String> = self
            .state
            .services
            .iter()
            .filter(|s| Self::INFRA_PROXIES.contains(&s.name.as_str()))
            .map(|s| s.name.clone())
            .collect();

        let mut body = div()
            .id("toxics-body")
            .flex()
            .flex_col()
            .w_full()
            .flex_grow()
            .gap(px(12.0))
            .px(px(20.0))
            .py(px(8.0))
            .overflow_y_scroll()
            .child(views::toxics::render_active_toxics_panel(
                &self.toxics.proxy_toxics,
            ));

        // XMTPD node latency controls
        let mut nodes_panel = ui::panel_container()
            .w_full()
            .child(ui::panel_title("XMTPD Nodes", theme::accent_blue()));

        if xmtpd_names.is_empty() {
            nodes_panel = nodes_panel.child(ui::empty_state("No XMTPD proxies found"));
        } else {
            for name in &xmtpd_names {
                nodes_panel = nodes_panel.child(self.render_proxy_latency_controls(name, cx));
            }
        }

        body = body.child(nodes_panel);

        // Infrastructure service latency controls
        let mut infra_panel = ui::panel_container()
            .w_full()
            .child(ui::panel_title("Infrastructure", theme::accent_mauve()));

        if infra_names.is_empty() {
            infra_panel = infra_panel.child(ui::empty_state("No infrastructure proxies found"));
        } else {
            for name in &infra_names {
                infra_panel = infra_panel.child(self.render_proxy_latency_controls(name, cx));
            }
        }

        body = body.child(infra_panel);
        body
    }

    fn render_proxy_latency_controls(
        &mut self,
        proxy_name: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let presets: &[(u32, &str)] = &[
            (50, "50ms"),
            (100, "100ms"),
            (250, "250ms"),
            (500, "500ms"),
            (1000, "1s"),
        ];

        let name_label: gpui::SharedString = proxy_name.to_string().into();

        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .py(px(4.0))
            .child(
                div()
                    .w(px(80.0))
                    .text_color(theme::text_primary())
                    .text_xs()
                    .child(name_label),
            );

        for &(ms, label) in presets {
            let proxy = proxy_name.to_string();
            let id: gpui::SharedString = format!("btn-{}-{}ms", proxy_name, ms).into();
            row = row.child(self.make_dynamic_button(
                id,
                label,
                ButtonVariant::Primary,
                self.busy,
                cx,
                move |view, _, _, cx| view.action_add_latency(proxy.clone(), ms, cx),
            ));
        }

        // Custom latency input + Set button
        let input = gpui_component::input::Input::new(&self.latency_input)
            .xsmall()
            .w(px(60.0));
        row = row.child(input);

        let proxy_for_custom = proxy_name.to_string();
        let custom_id: gpui::SharedString = format!("btn-{}-custom", proxy_name).into();
        row = row.child(self.make_dynamic_button(
            custom_id,
            "Set",
            ButtonVariant::Primary,
            self.busy,
            cx,
            move |view, _, _, cx| {
                let value = view.latency_input.read(cx).value().to_string();
                if let Ok(ms) = value.trim().parse::<u32>() {
                    view.action_add_latency(proxy_for_custom.clone(), ms, cx);
                }
            },
        ));

        // Reset button for this proxy
        let proxy_for_reset = proxy_name.to_string();
        let reset_id: gpui::SharedString = format!("btn-reset-{}", proxy_name).into();
        row = row.child(self.make_dynamic_button(
            reset_id,
            "Reset",
            ButtonVariant::Warning,
            self.busy,
            cx,
            move |view, _, _, cx| {
                view.action_reset_proxy(proxy_for_reset.clone(), cx);
            },
        ));

        row
    }

    // -- Toxics Action Handlers -----------------------------------------------

    fn action_add_latency(&mut self, proxy_name: String, latency_ms: u32, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        info!("Adding {}ms latency to {}", latency_ms, proxy_name);
        cx.notify();

        let result = Tokio::spawn(cx, actions::add_latency_toxic(proxy_name, latency_ms));

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = result.await?;
            cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.busy = false;
                    match result {
                        Ok(()) => info!("Latency added."),
                        Err(e) => {
                            let msg = e.to_string();
                            error!("{}", msg);
                            view.state.last_error = Some(msg);
                        }
                    }
                    cx.notify();
                    Ok::<_, Error>(())
                })
            })
        })
        .detach();
    }

    fn action_reset_proxy(&mut self, proxy_name: String, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        info!("Resetting toxics on {}", proxy_name);
        cx.notify();

        let result = Tokio::spawn(cx, actions::reset_proxy_toxics(proxy_name));

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = result.await?;
            cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.busy = false;
                    match result {
                        Ok(()) => info!("Proxy toxics reset."),
                        Err(e) => {
                            let msg = e.to_string();
                            error!("{}", msg);
                            view.state.last_error = Some(msg);
                        }
                    }
                    cx.notify();
                    Ok::<_, Error>(())
                })
            })
        })
        .detach();
    }

    fn action_reset_all_toxics(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        info!("Resetting all toxics…");
        cx.notify();

        let result = Tokio::spawn(cx, actions::reset_all_toxics());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = result.await?;
            cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.busy = false;
                    match result {
                        Ok(()) => info!("All toxics reset."),
                        Err(e) => {
                            let msg = e.to_string();
                            error!("{}", msg);
                            view.state.last_error = Some(msg);
                        }
                    }
                    cx.notify();
                    Ok::<_, Error>(())
                })
            })
        })
        .detach();
    }

    // -- Toxics Poller --------------------------------------------------------

    fn start_toxics_poller(&mut self, cx: &mut Context<Self>) {
        let poller = Tokio::spawn(cx, actions::start_toxics_poller());

        cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let (mut rx, abort_handle) = match poller.await? {
                Ok(pair) => pair,
                Err(e) => {
                    error!("Failed to start toxics poller: {e}");
                    return Ok(());
                }
            };
            let _ = cx.update(|cx| {
                let _ = this.update(cx, |view, _cx| {
                    view.poller_handles.push(abort_handle);
                });
            });
            while let Some(toxics) = rx.recv().await {
                let should_stop = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        if view.state.page != Page::Toxics {
                            return Ok::<_, Error>(true);
                        }
                        view.toxics.proxy_toxics = toxics;
                        cx.notify();
                        Ok(false)
                    })
                });
                if should_stop
                    .ok()
                    .and_then(|r| r.ok())
                    .and_then(|r| r.ok())
                    .unwrap_or(true)
                {
                    break;
                }
            }
            Ok::<_, Error>(())
        })
        .detach();
    }

    // -- Setup Dialog Overlay ------------------------------------------------

    /// Renders the setup dialog overlay with a Re-check button.
    fn render_setup_overlay(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let recheck_disabled = self.setup.rechecking;
        let label = if recheck_disabled {
            "Checking…"
        } else {
            "Re-check"
        };

        deferred(
            div()
                .id("setup-overlay")
                .absolute()
                .size_full()
                .top_0()
                .left_0()
                .occlude()
                .bg(hsla(0., 0., 0., 0.7))
                .flex()
                .flex_col()
                .justify_center()
                .items_center()
                .gap(px(16.0))
                .child(views::setup_dialog::render_setup_dialog(&self.setup))
                .child(self.make_clickable_button(
                    "btn-recheck",
                    label,
                    ButtonVariant::Primary,
                    recheck_disabled,
                    cx,
                    |view, _, _, cx| view.action_recheck(cx),
                )),
        )
        .with_priority(1000)
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::bg_primary())
            .child(views::header::render_header(self.state.network_status));

        match self.state.page {
            Page::Dashboard => {
                root = root
                    .child(views::log::render_error_bar(&self.state.last_error))
                    .child(self.render_toolbar(cx))
                    .child(
                        div()
                            .px(px(20.0))
                            .py(px(4.0))
                            .child(self.render_cutover_section(cx)),
                    )
                    .child(self.render_panels())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .max_h_1_3()
                            .px(px(20.0))
                            .pb(px(12.0))
                            .child(views::log::render_log_panel(
                                &self.state.log_lines,
                                &self.log_scroll,
                            )),
                    );
            }
            Page::Toxics => {
                root = root
                    .child(self.render_toxics_toolbar(cx))
                    .child(self.render_toxics_body(cx));
            }
        }

        // Show blocking overlay when setup checks haven't passed
        if self.setup.phase != SetupPhase::Ready {
            root = root.child(self.render_setup_overlay(cx));
        }

        root
    }
}
