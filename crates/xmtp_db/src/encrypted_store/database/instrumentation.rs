use diesel::connection::Instrumentation;
use diesel::connection::InstrumentationEvent;
use diesel::result::Error as DieselError;
use std::fmt::Write;

// Logs query errors. Panics on `database is locked` by default — a held lock in
// a pure Rust test is a real bug. Consumers with the client's retry machinery opt
// OUT via `XMTP_NO_PANIC_ON_DB_LOCK` (e.g. node-sdk Vitest workers, #3765).
#[allow(unused)]
pub struct TestInstrumentation;

/// Whether a `database is locked` error should panic. On by default; suppressed by
/// `XMTP_NO_PANIC_ON_DB_LOCK`.
fn panic_on_db_lock_enabled() -> bool {
    panic_on_db_lock_from_env(std::env::var("XMTP_NO_PANIC_ON_DB_LOCK"))
}

/// Pure helper (testable without mutating env): panic unless the var is `"1"`/`"true"`.
fn panic_on_db_lock_from_env(disable: Result<String, std::env::VarError>) -> bool {
    !matches!(disable, Ok(s) if s == "1" || s == "true")
}

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
                        if s.contains("database is locked") && panic_on_db_lock_enabled() {
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

#[cfg(test)]
mod tests {
    use super::panic_on_db_lock_from_env;
    use std::env::VarError;

    #[test]
    fn db_lock_panic_enabled_by_default() {
        assert!(panic_on_db_lock_from_env(Err(VarError::NotPresent)));
    }

    #[test]
    fn db_lock_panic_opt_out_values() {
        assert!(!panic_on_db_lock_from_env(Ok("1".to_string())));
        assert!(!panic_on_db_lock_from_env(Ok("true".to_string())));
    }

    #[test]
    fn db_lock_panic_ignores_other_values() {
        for v in ["0", "false", "", "yes", "TRUE", "True"] {
            assert!(
                panic_on_db_lock_from_env(Ok(v.to_string())),
                "value {v:?} should not disable the panic"
            );
        }
    }
}
