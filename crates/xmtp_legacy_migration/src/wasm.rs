//! Browser conversion owns a separate worker and SQL engine. Only the closed
//! source export uses the shared OPFS pool; migrations run in memory.
use crate::{
    InputError, MigrationError, MigrationReport, OutputError, PrepareMigrationArchiveArgs,
};
use diesel::{Connection, SqliteConnection};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/browser-storage.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readMigrationOutput)]
    async fn read_output(path: &str) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = writeMigrationOutput)]
    async fn write_output(path: &str, bytes: &[u8]) -> Result<JsValue, JsValue>;
}

fn storage(error: xmtp_db::StorageError) -> MigrationError {
    use xmtp_db::{
        StorageError,
        database::{OpfsSAHError, PlatformStorageError},
    };
    match error {
        StorageError::Platform(
            PlatformStorageError::DatabaseInUse
            | PlatformStorageError::SAH(OpfsSAHError::CreateSyncAccessHandle(_)),
        ) => MigrationError::SourceBusy,
        other => MigrationError::InvalidInput(InputError::Storage(other)),
    }
}

/// Export the exact OPFS source name before opening a separate in-memory copy.
pub(crate) async fn prepare(
    args: PrepareMigrationArchiveArgs,
) -> Result<MigrationReport, MigrationError> {
    if args.archive_key.len() != xmtp_archive::ENC_KEY_SIZE {
        return Err(MigrationError::invalid("archive key must contain 32 bytes"));
    }
    if args.database_key.is_some() {
        return Err(MigrationError::invalid(
            "legacy browser storage is unencrypted",
        ));
    }
    if args.output_path.is_empty() || args.output_path == args.database_path {
        return Err(MigrationError::invalid(
            "output must name a separate archive",
        ));
    }
    let mut bytes = xmtp_db::export_opfs_database(&args.database_path)
        .await
        .map_err(storage)?;
    if bytes.len() < 100 || !bytes.starts_with(b"SQLite format 3\0") {
        return Err(MigrationError::invalid("source is not a SQLite database"));
    }
    // The closed pool export has no separate WAL. Match its import contract.
    bytes[18] = 1;
    bytes[19] = 1;
    let mut conn = SqliteConnection::establish(":memory:")
        .map_err(|error| MigrationError::InvalidInput(InputError::Database(error)))?;
    conn.deserialize_database_from_buffer(&bytes)?;
    crate::migrations::apply(&mut conn)?;
    let mut archive = Vec::new();
    let mut writer = xmtp_archive::exporter::ElementWriter::new(&args.archive_key, &mut archive)?;
    let report = crate::records::export(&mut conn, args.output_path.clone(), |element| {
        writer.write(element).map_err(Into::into)
    })?;
    writer.finish()?;
    drop(conn);
    xmtp_db::database::pause_sqlite_if_idle();
    write_output(&args.output_path, &archive)
        .await
        .map_err(|value| MigrationError::Output(OutputError::Browser { value }))?;
    Ok(report)
}

pub(crate) async fn read(path: &str) -> Result<Vec<u8>, MigrationError> {
    let bytes = read_output(path)
        .await
        .map_err(|value| MigrationError::Output(OutputError::Browser { value }))?;
    Ok(js_sys::Uint8Array::new(&bytes).to_vec())
}
