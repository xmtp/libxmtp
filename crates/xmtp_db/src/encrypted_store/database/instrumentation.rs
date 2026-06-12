use diesel::connection::Instrumentation;
use diesel::connection::InstrumentationEvent;
use diesel::result::Error as DieselError;
use std::fmt::Write;

// Test instrumentation
// prints out query on error
// panics on database lock ONLY when explicitly opted in via the
// `XMTP_PANIC_ON_DB_LOCK` env var (accepts "1" or "true").
//
// `database is locked` is a transient, retryable condition in this codebase
// (SQLite is configured with `PRAGMA busy_timeout` and the error is classified
// as retryable). Panicking on it by default aborts the worker thread and crashes
// downstream test harnesses (e.g. the node-sdk Vitest fork workers), so the panic
// is opt-in for developers actively debugging lock contention.
#[allow(unused)]
pub struct TestInstrumentation;

/// Returns true when the caller has explicitly opted in to panicking on
/// `database is locked` via the `XMTP_PANIC_ON_DB_LOCK` env var.
fn panic_on_db_lock_enabled() -> bool {
    panic_on_db_lock_from_env(std::env::var("XMTP_PANIC_ON_DB_LOCK"))
}

/// Pure decision helper: the opt-in is enabled only when the env var resolves to
/// exactly `"1"` or `"true"`. Anything else (unset, empty, other values) keeps the
/// default non-panicking behavior. Kept separate from the env read so it can be
/// unit-tested without mutating the process environment.
fn panic_on_db_lock_from_env(var: Result<String, std::env::VarError>) -> bool {
    matches!(var, Ok(s) if s == "1" || s == "true")
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
    fn db_lock_panic_disabled_by_default() {
        // Unset env var (the default in CI / node-sdk Vitest workers) must NOT
        // opt in to panicking, so a `database is locked` error stays recoverable.
        assert!(!panic_on_db_lock_from_env(Err(VarError::NotPresent)));
    }

    #[test]
    fn db_lock_panic_opt_in_values() {
        assert!(panic_on_db_lock_from_env(Ok("1".to_string())));
        assert!(panic_on_db_lock_from_env(Ok("true".to_string())));
    }

    #[test]
    fn db_lock_panic_ignores_other_values() {
        for v in ["0", "false", "", "yes", "TRUE", "True"] {
            assert!(
                !panic_on_db_lock_from_env(Ok(v.to_string())),
                "value {v:?} should not enable the panic"
            );
        }
    }
}
