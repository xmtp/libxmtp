//! Browser runtime-control handle. Only the level filter is reloadable: file
//! logging and OTLP telemetry are not available in the browser, so there are no
//! worker guards to keep alive.

use tracing_subscriber::reload;
use tracing_subscriber::{EnvFilter, Registry};

use crate::config::Level;
use crate::error::Error;
use crate::filter::filter_directive;
use crate::layers::sink::{LogSinkTarget, SinkSlot};

/// Handle to the installed logging pipeline. The browser can change the
/// level filter and replace the log sink.
///
/// Created by [`crate::XmtpLoggingBuilder::install`].
pub struct LoggingHandle {
    filter: reload::Handle<EnvFilter, Registry>,
    sink: SinkSlot,
}

impl LoggingHandle {
    /// Build the wasm handle from the level-filter reload handle. Constructed by
    /// `install`; not public API.
    pub(crate) fn new(filter: reload::Handle<EnvFilter, Registry>, sink: SinkSlot) -> Self {
        Self { filter, sink }
    }

    /// Change the active log level for all libxmtp targets at runtime.
    pub fn set_level(&self, level: Level) -> Result<(), Error> {
        self.filter.reload(filter_directive(level.as_str()))?;
        Ok(())
    }

    /// No-op flush (no file/telemetry exporters in the browser).
    pub fn flush(&self) {}

    /// Replace or clear the browser's extra event sink.
    pub fn set_sink(&self, target: Option<std::sync::Arc<dyn LogSinkTarget>>) {
        self.sink.set_sink(target);
    }

    /// Count records rejected by the current sink.
    pub fn sink_error_count(&self) -> u64 {
        self.sink.error_count()
    }

    /// Count records rejected by a full browser delivery window.
    pub fn sink_dropped_count(&self) -> u64 {
        self.sink.dropped_count()
    }
}
