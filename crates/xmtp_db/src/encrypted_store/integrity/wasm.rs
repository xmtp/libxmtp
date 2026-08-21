//! Wasm by-path integrity checking over the OPFS SAH-pool VFS.
use super::*;
use crate::database::wasm::init_sqlite;
use diesel::connection::SimpleConnection;

/// Standalone integrity check of an OPFS-backed database. Wasm databases
/// are unencrypted, so there is no key parameter. Opens a second
/// connection through the shared SAH-pool VFS in this JS context.
pub async fn check_database_integrity(
    db_path: &str,
    level: IntegrityCheckLevel,
) -> IntegrityCheckResult {
    init_sqlite().await;
    // `mode=rw` (not `rwc`) drops SQLite's CREATE flag: a typo'd or
    // deleted OPFS path fails to open instead of being silently created
    // as an empty pool entry that would report a false `Ok`.
    let escaped_path = escape_uri_path(db_path);
    let mut conn = match SqliteConnection::establish(&format!("file:{escaped_path}?mode=rw")) {
        Ok(c) => c,
        Err(e) => {
            return IntegrityCheckResult::Failed {
                error: format!("failed to open {db_path}: {e}"),
            };
        }
    };
    // The checker must never mutate; mirror the native checker's guard.
    if let Err(e) = conn.batch_execute("PRAGMA query_only = ON;") {
        return classify_check_error(e);
    }
    run_checks(&mut conn, level, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{XmtpDb, XmtpTestDb};

    // This test binary (the crate's `--lib` unit tests) has no other wasm
    // test needing a browser, so without this it defaults to the Node.js
    // runner — which can't run OPFS (a browser-only API) and isn't even
    // installed in the wasm dev shell. Matches `tests/opfs.rs`'s setup.
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[xmtp_common::test(unwrap_try = true)]
    async fn wasm_static_and_store_checks_ok() {
        let db_path = xmtp_common::tmp_path();
        let store = crate::TestDb::create_persistent_store(Some(db_path.clone())).await;
        assert_eq!(
            store.integrity_check(IntegrityCheckLevel::Full)?,
            IntegrityCheckResult::Ok
        );
        drop(store);
        assert_eq!(
            check_database_integrity(&db_path, IntegrityCheckLevel::Quick).await,
            IntegrityCheckResult::Ok
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn wasm_missing_db_reports_failed_not_ok() {
        // A typo'd or deleted OPFS path must fail, not be silently created
        // as an empty database that reports a false Ok.
        let res = check_database_integrity(
            "nonexistent-integrity-probe.db3",
            IntegrityCheckLevel::Quick,
        )
        .await;
        assert!(
            matches!(res, IntegrityCheckResult::Failed { .. }),
            "got {res:?}"
        );
    }
}
