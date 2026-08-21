//! Read-only SQLite integrity checking (PRAGMA quick_check / integrity_check /
//! cipher_integrity_check). All entry points are non-mutating; the check
//! functions classify every failure into [`IntegrityCheckResult`] instead of
//! propagating errors, so callers can branch on a single enum.
use diesel::prelude::*;
use diesel::sql_query;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntegrityCheckLevel {
    /// `PRAGMA quick_check` — page/b-tree structure only, skips
    /// index↔table cross-validation. Fast.
    ///
    /// On encrypted (SQLCipher) databases this does NOT reliably detect
    /// ciphertext/page corruption — only [`IntegrityCheckLevel::Full`]'s
    /// `cipher_integrity_check` pass validates per-page HMACs.
    #[default]
    Quick,
    /// `PRAGMA integrity_check`, plus `PRAGMA cipher_integrity_check`
    /// (per-page HMAC validation) when the database is encrypted.
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrityCheckResult {
    /// All checks passed.
    Ok,
    /// One or more checks reported findings (rows other than "ok"), or a
    /// check failed with a corruption-shaped error.
    Corrupt { findings: Vec<String> },
    /// SQLITE_NOTADB-shaped failure: wrong key OR mangled header/ciphertext.
    /// Indistinguishable by design; both alert-worthy.
    Unreadable { reason: String },
    /// Encrypted database whose `.sqlcipher_salt` sidecar file is missing.
    SaltMissing,
    /// Could not obtain a read snapshot within busy_timeout.
    Locked,
    /// Filesystem or unexpected query failure.
    Failed { error: String },
}

/// Both built-in pragmas are queried through their table-valued form with an
/// alias, so one row-struct covers both.
#[derive(QueryableByName, Debug)]
struct CheckRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    result: String,
}

/// `PRAGMA cipher_integrity_check` is a SQLCipher extension pragma (no
/// table-valued form); its result column is named after the pragma.
#[cfg(not(target_arch = "wasm32"))]
#[derive(QueryableByName, Debug)]
struct CipherCheckRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    cipher_integrity_check: String,
}

fn quick_check(conn: &mut SqliteConnection) -> QueryResult<Vec<String>> {
    let rows =
        sql_query("SELECT quick_check AS result FROM pragma_quick_check").load::<CheckRow>(conn)?;
    Ok(rows
        .into_iter()
        .map(|r| r.result)
        .filter(|r| r != "ok")
        .collect())
}

fn integrity_check(conn: &mut SqliteConnection) -> QueryResult<Vec<String>> {
    let rows = sql_query("SELECT integrity_check AS result FROM pragma_integrity_check")
        .load::<CheckRow>(conn)?;
    Ok(rows
        .into_iter()
        .map(|r| r.result)
        .filter(|r| r != "ok")
        .collect())
}

/// Empty result set means every page's HMAC validated.
#[cfg(not(target_arch = "wasm32"))]
fn cipher_integrity_check(conn: &mut SqliteConnection) -> QueryResult<Vec<String>> {
    let rows = sql_query("PRAGMA cipher_integrity_check").load::<CipherCheckRow>(conn)?;
    Ok(rows.into_iter().map(|r| r.cipher_integrity_check).collect())
}

pub(crate) fn classify_check_error(e: diesel::result::Error) -> IntegrityCheckResult {
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    if lower.contains("malformed") || lower.contains("corrupt") {
        IntegrityCheckResult::Corrupt {
            findings: vec![msg],
        }
    } else if lower.contains("not a database") || lower.contains("sql logic error") {
        // SQLITE_NOTADB, or the bare SQLITE_ERROR fallback that page-decrypt
        // failures surface as under our plaintext-header setup. Both mean
        // "can't read this page with this key" — see `Unreadable`'s doc.
        IntegrityCheckResult::Unreadable { reason: msg }
    } else if lower.contains("database is locked") || lower.contains("database table is locked") {
        IntegrityCheckResult::Locked
    } else {
        IntegrityCheckResult::Failed { error: msg }
    }
}

/// Run the checks for `level` on an already-configured connection.
/// `encrypted` gates the SQLCipher HMAC pass on `Full` (native only).
pub(crate) fn run_checks(
    conn: &mut SqliteConnection,
    level: IntegrityCheckLevel,
    encrypted: bool,
) -> IntegrityCheckResult {
    let mut findings = Vec::new();
    let structural = match level {
        IntegrityCheckLevel::Quick => quick_check(conn),
        IntegrityCheckLevel::Full => integrity_check(conn),
    };
    match structural {
        Ok(f) => findings.extend(f),
        Err(e) => return classify_check_error(e),
    }
    #[cfg(not(target_arch = "wasm32"))]
    if encrypted && matches!(level, IntegrityCheckLevel::Full) {
        match cipher_integrity_check(conn) {
            Ok(f) => findings.extend(f),
            Err(e) => return classify_check_error(e),
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = encrypted;
    if findings.is_empty() {
        IntegrityCheckResult::Ok
    } else {
        IntegrityCheckResult::Corrupt { findings }
    }
}

/// Escape the URI delimiters SQLite would misparse in a `file:` open path
/// ('%' first — it is the escape introducer); otherwise a path containing
/// '?' or '#' would open a different file than the intended one.
fn escape_uri_path(path: &str) -> String {
    path.replace('%', "%25")
        .replace('?', "%3F")
        .replace('#', "%23")
}

#[cfg(not(target_arch = "wasm32"))]
pub use native_checks::check_database_integrity;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native_checks::check_database_path;

#[cfg(target_arch = "wasm32")]
pub use wasm_checks::check_database_integrity;

#[cfg(not(target_arch = "wasm32"))]
mod native_checks {
    use super::*;
    use crate::EncryptionKey;
    use crate::database::native::sqlcipher_connection::{assemble_session_pragmas, read_salt_hex};
    use diesel::connection::SimpleConnection;
    use xmtp_configuration::BUSY_TIMEOUT;

    fn missing_db(db_path: &str) -> IntegrityCheckResult {
        IntegrityCheckResult::Failed {
            error: format!("database file does not exist: {db_path}"),
        }
    }

    /// Open a dedicated read-only checking connection and run the checks.
    /// `session_pragmas` is the SQLCipher session setup for encrypted
    /// databases. Never creates, migrates, or writes to the database.
    pub(crate) fn check_database_path(
        db_path: &str,
        session_pragmas: Option<&str>,
        level: IntegrityCheckLevel,
    ) -> IntegrityCheckResult {
        if !std::path::Path::new(db_path).exists() {
            return missing_db(db_path);
        }
        // `mode=rw` (not `rwc`) never creates: a file deleted after the check
        // above fails to open instead of yielding an empty DB and a false Ok.
        // Read-write is still needed for WAL recovery on open.
        let escaped_path = escape_uri_path(db_path);
        let mut conn =
            match diesel::SqliteConnection::establish(&format!("file:{escaped_path}?mode=rw")) {
                Ok(c) => c,
                Err(e) => {
                    return IntegrityCheckResult::Failed {
                        error: format!("failed to open {db_path}: {e}"),
                    };
                }
            };
        if let Some(pragmas) = session_pragmas
            && let Err(e) = conn.batch_execute(pragmas)
        {
            return classify_check_error(e);
        }
        if let Err(e) = conn.batch_execute(&format!(
            "PRAGMA busy_timeout = {BUSY_TIMEOUT}; PRAGMA query_only = ON;"
        )) {
            return classify_check_error(e);
        }
        run_checks(&mut conn, level, session_pragmas.is_some())
    }

    /// Standalone integrity check of a database file, without a client.
    /// For encrypted databases pass the 32-byte encryption key; the
    /// SQLCipher salt is read from the `<db>.sqlcipher_salt` sidecar file.
    pub fn check_database_integrity(
        db_path: &str,
        key: Option<&EncryptionKey>,
        level: IntegrityCheckLevel,
    ) -> IntegrityCheckResult {
        // DB existence first: a mistyped path must report the missing
        // database, not `SaltMissing` (which implies the DB exists).
        if !std::path::Path::new(db_path).exists() {
            return missing_db(db_path);
        }
        let pragmas = match key {
            None => None,
            Some(key) => match read_salt_hex(db_path) {
                Ok(salt_hex) => Some(assemble_session_pragmas(key, Some(&salt_hex))),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return IntegrityCheckResult::SaltMissing;
                }
                Err(e) => {
                    return IntegrityCheckResult::Failed {
                        error: format!("salt sidecar: {e}"),
                    };
                }
            },
        };
        check_database_path(db_path, pragmas.as_deref(), level)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm_checks {
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
        // as an empty pool entry that would report a false `Ok`. Same
        // URI-delimiter escaping as the native checker.
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
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    // clippy's `unwrap_used` restriction lint only exempts functions
    // directly annotated `#[test]`/`#[tokio::test]`, not plain helpers
    // (like `mem_conn`/`create_encrypted_db` below) that tests call into —
    // matching the existing `test_util` module's override in `lib.rs`.
    #![allow(clippy::unwrap_used)]
    use super::*;
    use diesel::connection::SimpleConnection;

    fn mem_conn() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.batch_execute(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT); INSERT INTO t (v) VALUES ('a');",
        )
        .unwrap();
        conn
    }

    #[tokio::test]
    async fn quick_and_full_ok_on_healthy_db() {
        let mut conn = mem_conn();
        assert_eq!(
            run_checks(&mut conn, IntegrityCheckLevel::Quick, false),
            IntegrityCheckResult::Ok
        );
        assert_eq!(
            run_checks(&mut conn, IntegrityCheckLevel::Full, false),
            IntegrityCheckResult::Ok
        );
    }

    #[tokio::test]
    async fn classify_maps_error_strings() {
        use diesel::result::{DatabaseErrorKind, Error};
        fn db_err(msg: &str) -> Error {
            Error::DatabaseError(DatabaseErrorKind::Unknown, Box::new(msg.to_string()))
        }
        assert!(matches!(
            classify_check_error(db_err("database disk image is malformed")),
            IntegrityCheckResult::Corrupt { .. }
        ));
        assert!(matches!(
            classify_check_error(db_err("file is not a database")),
            IntegrityCheckResult::Unreadable { .. }
        ));
        assert!(matches!(
            classify_check_error(db_err("database is locked")),
            IntegrityCheckResult::Locked
        ));
        assert!(matches!(
            classify_check_error(db_err("sql logic error")),
            IntegrityCheckResult::Unreadable { .. }
        ));
        assert!(matches!(
            classify_check_error(db_err("no such table: whatever")),
            IntegrityCheckResult::Failed { .. }
        ));
    }

    use crate::{EncryptedMessageStore, NativeDb};
    use std::io::{Seek, SeekFrom, Write};

    /// Build a real encrypted store at `db_path`, write schema, close it.
    /// Forces a full WAL checkpoint before closing so all data lands in the
    /// main file — a fresh WAL-mode store otherwise leaves the migrated
    /// schema sitting in the `-wal` sidecar, which a byte-flipping test
    /// against the main file would never touch (SQLite transparently reads
    /// through the WAL on open).
    fn create_encrypted_db(db_path: &str) -> crate::EncryptionKey {
        use crate::ConnectionExt;

        let key = EncryptedMessageStore::<()>::generate_enc_key();
        let db = NativeDb::builder()
            .persistent(db_path.to_string())
            .key(key)
            .build()
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        store
            .conn()
            .raw_query(|c| c.batch_execute("PRAGMA wal_checkpoint(TRUNCATE);"))
            .unwrap();
        drop(store);
        key.into()
    }

    #[tokio::test]
    async fn by_path_ok_on_healthy_encrypted_db() {
        let db_path = xmtp_common::tmp_path();
        let key = create_encrypted_db(&db_path);
        for level in [IntegrityCheckLevel::Quick, IntegrityCheckLevel::Full] {
            assert_eq!(
                check_database_integrity(&db_path, Some(&key), level),
                IntegrityCheckResult::Ok,
                "level {level:?}"
            );
        }
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn by_path_detects_corruption() {
        let db_path = xmtp_common::tmp_path();
        let key = create_encrypted_db(&db_path);
        // Flip bytes in the middle of the file (past the 32-byte plaintext
        // header, inside encrypted page data) so the page's HMAC fails.
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&db_path)
            .unwrap();
        let len = f.metadata().unwrap().len();
        f.seek(SeekFrom::Start(len / 2)).unwrap();
        f.write_all(&[0xFF; 512]).unwrap();
        drop(f);

        // Only `Full` is asserted here. `Full` always catches it:
        // `cipher_integrity_check` walks every page the header declares
        // (1..page_count) and verifies its HMAC directly, so a flipped
        // ciphertext byte anywhere in the file is always caught — this was
        // empirically 100% reliable across dozens of runs with several
        // corruption strategies (single-offset flip, whole-file flip,
        // truncation).
        //
        // `quick_check`/`integrity_check` (SQLite's own structural pass,
        // used by both levels) only walk pages reachable from the live
        // b-tree structure and were observed to *not* reliably re-verify
        // a page's HMAC on every read in this SQLCipher build (4.6.1
        // community / OpenSSL 3.6.3) — sometimes catching a corrupted page
        // 1, sometimes not, with no code change between runs. That matches
        // `IntegrityCheckLevel::Quick`'s own doc comment (fast, no HMAC
        // pass) versus `Full`'s explicit addition of
        // `cipher_integrity_check` for encrypted databases: Quick is not a
        // tamper-detection guarantee, only Full's cipher_integrity_check
        // is. Asserting Quick here would be asserting behavior the type's
        // own contract doesn't promise. `by_path_wrong_key_is_unreadable`
        // separately confirms Quick reliably detects a *wrong key* (every
        // page — including page 1, which quick_check must read to resolve
        // the schema — fails to decrypt), which is the corruption shape
        // Quick *is* documented to catch.
        let res = check_database_integrity(&db_path, Some(&key), IntegrityCheckLevel::Full);
        assert!(
            matches!(
                res,
                IntegrityCheckResult::Corrupt { .. } | IntegrityCheckResult::Unreadable { .. }
            ),
            "got {res:?}"
        );
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn by_path_wrong_key_is_unreadable() {
        let db_path = xmtp_common::tmp_path();
        let _key = create_encrypted_db(&db_path);
        let wrong: crate::EncryptionKey = EncryptedMessageStore::<()>::generate_enc_key().into();
        let res = check_database_integrity(&db_path, Some(&wrong), IntegrityCheckLevel::Quick);
        assert!(
            matches!(res, IntegrityCheckResult::Unreadable { .. }),
            "got {res:?}"
        );
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn by_path_missing_db_reports_missing_db_not_salt() {
        // A nonexistent encrypted DB (no db file, no sidecar) must report
        // the missing database, not SaltMissing.
        let db_path = xmtp_common::tmp_path();
        let key = EncryptedMessageStore::<()>::generate_enc_key().into();
        let res = check_database_integrity(&db_path, Some(&key), IntegrityCheckLevel::Quick);
        assert!(
            matches!(&res, IntegrityCheckResult::Failed { error } if error.contains("does not exist")),
            "got {res:?}"
        );
    }

    #[tokio::test]
    async fn by_path_rejects_malformed_salt_sidecar() {
        let db_path = xmtp_common::tmp_path();
        let key = create_encrypted_db(&db_path);
        let salt = crate::database::EncryptedConnection::salt_file(&db_path).unwrap();
        let mut perms = std::fs::metadata(&salt).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&salt, perms).unwrap();
        // Anything that isn't exactly 32 hex chars must be rejected before
        // interpolation — including SQL-injection-shaped contents, since the
        // session pragmas run before `query_only = ON`.
        for bad in [
            "",
            "zz",
            "0123456789abcdef0123456789abcde",   // 31 chars
            "0123456789abcdef0123456789abcdef0", // 33 chars
            "x'00'\"; DROP TABLE user_preferences; --",
        ] {
            std::fs::write(&salt, bad).unwrap();
            let res = check_database_integrity(&db_path, Some(&key), IntegrityCheckLevel::Quick);
            assert!(
                matches!(&res, IntegrityCheckResult::Failed { error } if error.contains("salt")),
                "salt {bad:?} got {res:?}"
            );
        }
        // A well-formed (but wrong) salt still goes through and fails as
        // unreadable, not as a sidecar error.
        std::fs::write(&salt, "00000000000000000000000000000000").unwrap();
        let res = check_database_integrity(&db_path, Some(&key), IntegrityCheckLevel::Quick);
        assert!(
            matches!(res, IntegrityCheckResult::Unreadable { .. }),
            "got {res:?}"
        );
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    // Unix-only: Windows forbids '?' in filenames, so the fixture below
    // could not be created there at all. The escaping it exercises is
    // platform-independent (`escape_uri_path`); only the fixture is not.
    #[cfg(unix)]
    #[tokio::test]
    async fn by_path_handles_uri_delimiters_in_path() {
        // '?', '#', and '%' are legal filename bytes on Unix but URI
        // delimiters/escapes in SQLite's `file:` open path — the checker
        // must open the same file it validated.
        let dir = xmtp_common::tmp_path();
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = format!("{dir}/we?ird#100%.db3");
        let key = create_encrypted_db(&db_path);
        assert_eq!(
            check_database_integrity(&db_path, Some(&key), IntegrityCheckLevel::Full),
            IntegrityCheckResult::Ok
        );
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn by_path_missing_salt_file() {
        let db_path = xmtp_common::tmp_path();
        let key = create_encrypted_db(&db_path);
        let salt = crate::database::EncryptedConnection::salt_file(&db_path).unwrap();
        // salt file is write-protected; lift perms then remove
        let mut perms = std::fs::metadata(&salt).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&salt, perms).unwrap();
        std::fs::remove_file(&salt).unwrap();
        assert_eq!(
            check_database_integrity(&db_path, Some(&key), IntegrityCheckLevel::Quick),
            IntegrityCheckResult::SaltMissing
        );
        // Not `remove_db_files`: it unconditionally tries to delete the salt
        // sidecar too, which this test already removed.
        std::fs::remove_file(&db_path).unwrap();
    }

    #[tokio::test]
    async fn by_path_unencrypted_db() {
        let db_path = xmtp_common::tmp_path();
        {
            let db = NativeDb::builder()
                .persistent(db_path.clone())
                .build_unencrypted()
                .unwrap();
            let _store = EncryptedMessageStore::new(db).unwrap();
        }
        assert_eq!(
            check_database_integrity(&db_path, None, IntegrityCheckLevel::Full),
            IntegrityCheckResult::Ok
        );
        // Not `remove_db_files`: an unencrypted db has no salt sidecar file.
        std::fs::remove_file(&db_path).unwrap();
    }

    #[tokio::test]
    async fn full_check_does_not_block_concurrent_writer() {
        let db_path = xmtp_common::tmp_path();
        let key = create_encrypted_db(&db_path);
        let db = NativeDb::builder()
            .persistent(db_path.clone())
            .key(key.clone())
            .build()
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();

        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let written = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        // The writer loop only exits when `stop` is set. Setting it on the
        // success path alone would turn any failed assertion below into a
        // hang: the panic unwinds past the `stop.store`, the blocking task
        // spins forever, and tokio's runtime drop waits on it. Stopping in
        // `Drop` bounds the failure instead.
        struct StopOnDrop(Arc<AtomicBool>);
        impl Drop for StopOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }
        let _stop_guard = StopOnDrop(stop.clone());
        let (writer_written, writer_stop) = (written.clone(), stop.clone());
        let writer = tokio::task::spawn_blocking(move || {
            use crate::ConnectionExt;
            let db = store;
            // `user_preferences` is a singleton row (CHECK id = 0); use
            // `key_package_history` instead — it accepts unlimited rows and
            // only requires a unique blob + a timestamp, so every insert
            // succeeds and genuinely exercises concurrent WAL writes.
            let mut i = 0i64;
            while !writer_stop.load(Ordering::Relaxed) {
                db.conn()
                    .raw_query(|c| {
                        diesel::sql_query(format!(
                            "INSERT INTO key_package_history (key_package_hash_ref, created_at_ns) VALUES (randomblob(32), {})",
                            1_700_000_000_000i64 + i
                        ))
                        .execute(c)
                    })
                    .unwrap();
                i += 1;
                writer_written.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Gate each check on observed writer progress since the previous
        // check, so a check can only run while the writer is demonstrably
        // mid-stream — the writer/check overlap is guaranteed, not
        // coincidental. If a Full check blocked WAL writers, the progress
        // wait after it would hang and the deadline below would fire.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        for _ in 0..5 {
            let before = written.load(Ordering::Relaxed);
            while written.load(Ordering::Relaxed) <= before {
                assert!(
                    std::time::Instant::now() < deadline,
                    "writer made no progress: a check appears to block WAL writers"
                );
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            let res = check_database_integrity(&db_path, Some(&key), IntegrityCheckLevel::Full);
            assert!(
                matches!(res, IntegrityCheckResult::Ok | IntegrityCheckResult::Locked),
                "got {res:?}"
            );
        }
        stop.store(true, Ordering::Relaxed);
        tokio::time::timeout(std::time::Duration::from_secs(30), writer)
            .await
            .expect("writer did not finish after checks — blocked by checker?")
            .unwrap();
        // After the writer finishes, the check must be cleanly Ok.
        assert_eq!(
            check_database_integrity(&db_path, Some(&key), IntegrityCheckLevel::Full),
            IntegrityCheckResult::Ok
        );
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn store_level_check_uses_dedicated_connection() {
        use crate::XmtpDb;
        let db_path = xmtp_common::tmp_path();
        let key = create_encrypted_db(&db_path);
        let db = NativeDb::builder()
            .persistent(db_path.clone())
            .key(key)
            .build()
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        assert_eq!(
            store.integrity_check(IntegrityCheckLevel::Full).unwrap(),
            IntegrityCheckResult::Ok
        );
        drop(store);
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn ephemeral_check_runs_on_existing_connection() {
        use crate::XmtpDb;
        // `NativeDb::builder().ephemeral(true)` from the brief doesn't match
        // this codebase's builder API: `ephemeral()` takes no argument, and
        // `.build()` requires a key to be set (or `.build_unencrypted()`).
        // Existing ephemeral tests (e.g. `test_utils.rs`) use
        // `.ephemeral().build_unencrypted()`; matched here.
        let db = NativeDb::builder().ephemeral().build_unencrypted().unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        assert_eq!(
            store.integrity_check(IntegrityCheckLevel::Quick).unwrap(),
            IntegrityCheckResult::Ok
        );
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
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
