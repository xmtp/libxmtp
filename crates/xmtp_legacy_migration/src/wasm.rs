//! Browser conversion owns a separate worker and SQL engine. The closed
//! source is copied in chunks; migrations run on a private OPFS working copy.
use crate::{
    InputError, MigrationError, MigrationReport, OutputError, PrepareMigrationArchiveArgs,
};
use diesel::{Connection, SqliteConnection, connection::SimpleConnection};
use std::io::{self, Write};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/browser-storage.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readMigrationOutput)]
    async fn read_output(path: &str) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = beginMigrationOutput)]
    async fn begin_output(path: &str) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = writeMigrationChunk)]
    fn write_chunk(output: &JsValue, bytes: &[u8]) -> Result<usize, JsValue>;
    #[wasm_bindgen(catch, js_name = flushMigrationOutput)]
    fn flush_output(output: &JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = abortMigrationOutput)]
    fn abort_output(output: &JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = discardMigrationOutput)]
    async fn discard_output(output: &JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = commitMigrationOutput)]
    async fn commit_output(output: &JsValue) -> Result<(), JsValue>;
}

// Limit each JavaScript transfer. The sink retains no archive chunks.
const OUTPUT_CHUNK_BYTES: usize = 64 * 1024;
struct Output(JsValue);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        write_chunk(&self.0, &bytes[..bytes.len().min(OUTPUT_CHUNK_BYTES)])
            .map_err(|error| io::Error::other(format!("{error:?}")))
    }
    fn flush(&mut self) -> io::Result<()> {
        flush_output(&self.0).map_err(|error| io::Error::other(format!("{error:?}")))
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        // The next access removes any file that was not published.
        let _ = abort_output(&self.0);
    }
}
fn output(error: JsValue) -> MigrationError {
    MigrationError::Output(OutputError::Browser { value: error })
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

fn working_error(error: xmtp_db::database::OpfsWorkingCopyError) -> MigrationError {
    use xmtp_db::database::OpfsWorkingCopyError;
    match error {
        OpfsWorkingCopyError::Source(source) => storage(source),
        error @ (OpfsWorkingCopyError::Output(_) | OpfsWorkingCopyError::OutputIo(_)) => {
            MigrationError::Output(OutputError::WorkingCopy(error))
        }
        error => MigrationError::InvalidInput(InputError::WorkingCopy(error)),
    }
}

/// Copy raw OPFS bytes to a private, disk-backed VFS before running SQLite.
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
    let working = xmtp_db::database::OpfsWorkingCopy::new(&args.database_path)
        .await
        .map_err(working_error)?;
    let mut conn = SqliteConnection::establish(&working.database_uri())
        .map_err(|error| MigrationError::InvalidInput(InputError::Database(error)))?;
    conn.batch_execute(
        "PRAGMA journal_mode=DELETE; PRAGMA cache_size=-2048; PRAGMA temp_store=FILE;",
    )?;
    // Browser cancellation terminates the worker.
    crate::migrations::apply(&mut conn, || Ok(()))?;
    let mut sink = Output(begin_output(&args.output_path).await.map_err(output)?);
    let result = (|| {
        let mut writer = xmtp_archive::exporter::ElementWriter::new(&args.archive_key, &mut sink)?;
        let report = crate::records::export(&mut conn, args.output_path.clone(), |element| {
            writer.write(element).map_err(Into::into)
        })?;
        writer.finish()?;
        Ok::<_, MigrationError>(report)
    })();
    drop(conn);
    let cleanup = working.finish().map_err(working_error);
    xmtp_db::database::pause_sqlite_if_idle();
    let report = match result {
        Ok(report) => report,
        Err(error) => {
            // Keep the conversion error if cleanup also fails.
            let _ = discard_output(&sink.0).await;
            return Err(error);
        }
    };
    if let Err(error) = cleanup {
        let _ = discard_output(&sink.0).await;
        return Err(error);
    }
    commit_output(&sink.0).await.map_err(output)?;
    Ok(report)
}

pub(crate) async fn read(path: &str) -> Result<Vec<u8>, MigrationError> {
    let bytes = read_output(path)
        .await
        .map_err(|value| MigrationError::Output(OutputError::Browser { value }))?;
    Ok(js_sys::Uint8Array::new(&bytes).to_vec())
}
