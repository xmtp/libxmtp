//! Application state shared across the GUI.
//!
//! `AppState` is an observable entity that tracks running services, xmtpd nodes,
//! and the status of the network.  Background operations (up / down / delete /
//! add-node) are dispatched onto the tokio runtime via [`gpui::App::spawn`] and
//! update this state when they complete.

use std::sync::Arc;

/// The high-level lifecycle of the xnet network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkStatus {
    /// No services have been started yet (or they were deleted).
    Stopped,
    /// Services are being brought up.
    Starting,
    /// All core services are running.
    Running,
    /// Services are being stopped.
    Stopping,
    /// Services and containers are being deleted.
    Deleting,
    /// Something went wrong – the message is stored in `AppState::last_error`.
    Error,
}

/// Minimal description of a running service for the info panel.
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub status: &'static str,
    pub external_url: Option<String>,
}

/// Description of an xmtpd node for the info panel.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: u32,
    pub container_name: String,
    pub url: String,
}

/// Status of a single dependency check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// Check has not run yet.
    Pending,
    /// Check is currently running.
    Checking,
    /// Check passed.
    Passed,
    /// Check failed.
    Failed,
}

/// Which phase of the setup flow the app is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupPhase {
    /// Checking Docker at startup.
    Docker,
    /// Checking DNS after "Up" completes.
    Dns,
    /// All checks passed — app is fully usable.
    Ready,
}

/// State for the startup dependency checks.
#[derive(Debug, Clone)]
pub struct SetupChecks {
    pub phase: SetupPhase,
    pub docker: CheckStatus,
    pub dns: CheckStatus,
    pub docker_error: Option<String>,
    pub dns_error: Option<String>,
    pub rechecking: bool,
    /// Frame index for the animated spinner shown during checks.
    pub spinner_tick: usize,
}

impl SetupChecks {
    pub fn new() -> Self {
        Self {
            phase: SetupPhase::Docker,
            docker: CheckStatus::Pending,
            dns: CheckStatus::Pending,
            docker_error: None,
            dns_error: None,
            rechecking: false,
            spinner_tick: 0,
        }
    }
}

/// Which page the app is currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// The main dashboard (services, nodes, logs).
    Dashboard,
    /// The ToxiProxy latency management page.
    Toxics,
}

/// A single toxic displayed in the UI, flattened from ToxicPack.
#[derive(Debug, Clone)]
pub struct ToxicInfo {
    pub proxy_name: String,
    pub toxic_type: String,
    pub stream: String,
    pub toxicity: f32,
    pub latency: Option<u32>,
    pub jitter: Option<u32>,
}

/// State for the toxics management page.
#[derive(Debug, Clone)]
pub struct ToxicsState {
    pub proxy_toxics: Vec<ToxicInfo>,
}

impl ToxicsState {
    pub fn new() -> Self {
        Self {
            proxy_toxics: Vec::new(),
        }
    }
}

/// Central application state.
#[derive(Debug, Clone)]
pub struct AppState {
    pub network_status: NetworkStatus,
    pub services: Vec<ServiceInfo>,
    pub xmtpd_nodes: Vec<NodeInfo>,
    pub last_error: Option<String>,
    pub log_lines: Vec<Arc<str>>,
    pub page: Page,
    pub cutover_ns: Option<u64>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            network_status: NetworkStatus::Stopped,
            services: Vec::new(),
            xmtpd_nodes: Vec::new(),
            last_error: None,
            log_lines: Vec::new(),
            page: Page::Dashboard,
            cutover_ns: None,
        }
    }

    pub fn push_log(&mut self, msg: impl Into<Arc<str>>) {
        self.log_lines.push(msg.into());
        // Keep a rolling window so memory stays bounded.
        if self.log_lines.len() > 200 {
            self.log_lines.drain(..self.log_lines.len() - 200);
        }
    }
}
