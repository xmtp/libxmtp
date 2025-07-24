use diesel::connection::Instrumentation;
use diesel::connection::InstrumentationEvent;

pub struct InstrumentedSqliteConnection;

impl Instrumentation for InstrumentedSqliteConnection {
    fn on_connection_event(&mut self, event: InstrumentationEvent<'_>) {
        use InstrumentationEvent::*;
        match event {
            // StartQuery { query, .. } => tracing::debug!("StartQuery {{ {} }}", query),
            FinishQuery { query, error, .. } => {
                tracing::debug!("FinishQuery {{ {} {:?} }}", query, error)
            }
            _ => (),
        }
    }
}
