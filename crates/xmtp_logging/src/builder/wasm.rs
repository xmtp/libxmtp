//! Browser builder surface: an `install` that chains the console + optional
//! performance layers. File logging and telemetry are not available in the
//! browser, so there are no `with_file` / `with_telemetry` knobs here.

use super::XmtpLoggingBuilder;
use crate::error::Error;
use crate::handle::LoggingHandle;

impl XmtpLoggingBuilder {
    /// Install the global subscriber (wasm). Only the level filter is reloadable;
    /// file logging and telemetry are not available in the browser.
    pub fn install(self) -> Result<LoggingHandle, Error> {
        use crate::filter::filter_directive;
        use crate::layers::web::{console_layer, perf_layer};
        use tracing_subscriber::prelude::*;
        use tracing_subscriber::reload;

        let cfg = self.cfg;

        // Unlike native, the wasm layers are chained with `.with(..)` instead of
        // collected into a `Vec<Box<dyn Layer>>`. The browser layers are not
        // `Send + Sync`, and `tracing-subscriber` only implements `Layer` for
        // `Box<dyn Layer + Send + Sync>` — so a boxed `Vec` of them has no `Layer`
        // impl. Chaining the concrete (unboxed) layers sidesteps that entirely.
        // Only the level filter is reloadable; `console`/`perf` are fixed, and the
        // optional perf layer rides through as an `Option<impl Layer>` (which is
        // itself a `Layer`). The layer type parameters are left to inference: each
        // `.with(..)` re-parameterizes the subscriber, so the layers must bind to
        // the accumulated `Layered<..>` type rather than to a pinned `Registry`.
        let (filter_layer, filter_handle) =
            reload::Layer::new(filter_directive(cfg.level.as_str()));

        let perf = cfg.performance.then(|| perf_layer());

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(console_layer())
            .with(perf)
            .try_init()
            .map_err(|_| Error::AlreadyInitialized)?;

        Ok(LoggingHandle::new(filter_handle))
    }
}
