use diesel::connection::Instrumentation;
use diesel::connection::InstrumentationEvent;
use diesel::result::Error as DieselError;
use std::fmt::Write;

// Test instrumentatiomn
// prints out query on error
// panics on database lock
#[allow(unused)]
pub struct TestInstrumentation;

impl Instrumentation for TestInstrumentation {
    fn on_connection_event(&mut self, event: InstrumentationEvent<'_>) {
        use InstrumentationEvent::*;
        match event {
            FinishQuery { query, error, .. } => {
                if let Some(e) = error {
                    tracing::error!("query {} errored with {:?}", query, error);
                    if let DieselError::DatabaseError(_, info) = e {
                        let mut s = String::new();
                        let _ = write!(s, "{},", info.message());
                        if let Some(name) = info.table_name() {
                            let _ = write!(s, "table_name={},", name);
                        }
                        if let Some(name) = info.column_name() {
                            let _ = write!(s, "column_name={},", name);
                        }
                        if let Some(hint) = info.hint() {
                            let _ = write!(s, "hint={},", hint);
                        }
                        if let Some(details) = info.details() {
                            tracing::error!("details: {},", details);
                        }
                        tracing::error!("{}", s);
                        if s.contains("database is locked") {
                            panic!("database locked");
                        }
                    }
                }
            }
            BeginTransaction { depth, .. } => {
                tracing::trace!("Begin Transaction @ depth={}", depth);
            }
            CommitTransaction { depth, .. } => {
                tracing::trace!("Commit Transaction @ depth={}", depth);
            }
            RollbackTransaction { depth, .. } => {
                tracing::trace!("Rollback Transaction @ depth={}", depth);
            }

            _ => (),
        }
    }
}
