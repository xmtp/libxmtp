use super::*;
use crate::xmtp_openmls_provider::XmtpOpenMlsProviderPrivate;

use diesel::connection::SimpleConnection;
use diesel_migrations::MigrationHarness;

#[derive(Clone, Debug)]
/// Manages a Sqlite db for persisting messages and other objects.
pub struct EncryptedMessageStore<Db> {
    pub(super) opts: StorageOption,
    pub(super) db: Db,
}

impl<Db, E> EncryptedMessageStore<Db>
where
    Db: XmtpDb<Error = E>,
    StorageError: From<E>,
{
    #[tracing::instrument(level = "debug", skip_all)]
    pub(super) fn init_db(&mut self) -> Result<(), StorageError> {
        self.db.validate(&self.opts)?;
        self.db.conn()?.raw_query_write(|conn| {
            conn.batch_execute("PRAGMA journal_mode = WAL;")?;
            tracing::info!("Running DB migrations");
            conn.run_pending_migrations(MIGRATIONS)?;

            let sqlite_version =
                sql_query("SELECT sqlite_version() AS version").load::<SqliteVersion>(conn)?;
            tracing::info!("sqlite_version={}", sqlite_version[0].version);

            tracing::info!("Migrations successful");
            Ok(())
        })?;

        Ok::<_, StorageError>(())
    }

    pub fn mls_provider(&self) -> Result<XmtpOpenMlsProviderPrivate<Db::Connection>, StorageError> {
        let conn = self.conn()?;
        Ok(XmtpOpenMlsProviderPrivate::new(conn))
    }

    /// Pulls a new connection from the store
    pub fn conn(&self) -> Result<impl ConnectionExt, StorageError> {
        Ok(self.db.conn()?)
    }

    /// Release connection to the database, closing it
    pub fn release_connection(&self) -> Result<(), StorageError> {
        Ok(self.db.reconnect()?)
    }

    /// Reconnect to the database
    pub fn reconnect(&self) -> Result<(), StorageError> {
        Ok(self.db.reconnect()?)
    }
}
