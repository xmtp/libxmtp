use std::{io, sync::Arc};

use parking_lot::Mutex;
use tracing::Dispatch;
use tracing_subscriber::{Layer, fmt, layer::SubscriberExt};

use crate::{Level, filter_directive};

#[cfg(test)]
mod tests;

/// Capture JSON events through the production filter without changing global logging.
///
/// Run the tested future with this dispatch via `WithSubscriber`, or use
/// `tracing::dispatcher::with_default` for synchronous work. Spawned tasks must
/// carry the same dispatch when their events belong to the test.
#[derive(Clone)]
pub struct LogCapture {
    output: Buffer,
    dispatch: Dispatch,
}

impl LogCapture {
    /// Create an isolated capture at the same level used by the production pipeline.
    pub fn new(level: Level) -> Self {
        let output = Buffer::default();
        let layer = fmt::layer()
            .json()
            .flatten_event(true)
            .with_ansi(false)
            .with_writer(output.clone())
            .with_filter(filter_directive(level.as_str()));
        let dispatch = Dispatch::new(tracing_subscriber::registry().with(layer));
        Self { output, dispatch }
    }

    /// Return a scoped dispatcher; this does not install a global subscriber.
    pub fn dispatch(&self) -> Dispatch {
        self.dispatch.clone()
    }

    /// Read complete JSON lines captured so far.
    pub fn output(&self) -> String {
        String::from_utf8(self.output.0.lock().clone()).expect("JSON logs are UTF-8")
    }
}

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl io::Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> fmt::MakeWriter<'a> for Buffer {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
