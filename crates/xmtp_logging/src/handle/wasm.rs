//! Browser runtime-control handle. Only the level filter is reloadable: file
//! logging and OTLP telemetry are not available in the browser, so there are no
//! worker guards to keep alive.

use tracing_subscriber::reload;
use tracing_subscriber::{EnvFilter, Registry};

use crate::config::Level;
use crate::error::Error;
use crate::filter::filter_directive;

/// Handle to the installed logging pipeline. In the browser the only
/// runtime-mutable slot is the level filter.
///
/// Created by [`crate::XmtpLoggingBuilder::install`].
pub struct LoggingHandle {
    filter: reload::Handle<EnvFilter, Registry>,
}

impl LoggingHandle {
    /// Build the wasm handle from the level-filter reload handle. Constructed by
    /// `install`; not public API.
    pub(crate) fn new(filter: reload::Handle<EnvFilter, Registry>) -> Self {
        Self { filter }
    }

    /// Change the active log level for all libxmtp targets at runtime.
    pub fn set_level(&self, level: Level) -> Result<(), Error> {
        self.filter.reload(filter_directive(level.as_str()))?;
        Ok(())
    }

    /// No-op flush (no file/telemetry exporters in the browser).
    pub fn flush(&self) {}
}
