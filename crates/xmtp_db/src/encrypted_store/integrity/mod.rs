//! Read-only SQLite integrity checking (PRAGMA quick_check / integrity_check /
//! cipher_integrity_check). No entry point creates, migrates, or writes user
//! data (opening may run SQLite's standard crash recovery — see
//! `check_database_path`); every failure is classified into
//! [`IntegrityCheckResult`] instead of propagated, so callers branch on one enum.
use diesel::prelude::*;
use diesel::sql_query;

xmtp_common::if_native! {
    mod native;
    pub use native::check_database_integrity;
    pub(crate) use native::check_database_path;
}

xmtp_common::if_wasm! {
    mod wasm;
    pub use wasm::check_database_integrity;
}

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
    xmtp_common::if_native! {@
        if encrypted && matches!(level, IntegrityCheckLevel::Full) {
            match native::cipher_integrity_check(conn) {
                Ok(f) => findings.extend(f),
                Err(e) => return classify_check_error(e),
            }
        }
    }
    xmtp_common::if_wasm! {@
        let _ = encrypted;
    }
    if findings.is_empty() {
        IntegrityCheckResult::Ok
    } else {
        IntegrityCheckResult::Corrupt { findings }
    }
}

/// Escape the URI delimiters SQLite would misparse in a `file:` open path
/// ('%' first — it is the escape introducer); otherwise a path containing
/// '?' or '#' would open a different file than the intended one.
/// On Windows, also normalize to SQLite's URI form: forward slashes, and a
/// leading '/' before drive-letter absolute paths (`file:/C:/...`).
fn escape_uri_path(path: &str) -> String {
    let escaped = path
        .replace('%', "%25")
        .replace('?', "%3F")
        .replace('#', "%23");
    #[cfg(windows)]
    {
        let escaped = escaped.replace('\\', "/");
        let mut chars = escaped.chars();
        if chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':') {
            return format!("/{escaped}");
        }
        return escaped;
    }
    #[cfg(not(windows))]
    escaped
}
