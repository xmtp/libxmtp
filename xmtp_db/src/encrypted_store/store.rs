use super::*;
use crate::xmtp_openmls_provider::XmtpOpenMlsProvider;

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
    StorageError: From<<<Db as XmtpDb>::Connection as ConnectionExt>::Error>,
    StorageError: From<E>,
{
    #[tracing::instrument(level = "debug", skip_all)]
    pub(super) fn init_db(&mut self) -> Result<(), StorageError> {
        self.db.validate(&self.opts)?;
        self.db.conn().raw_query_write::<_, _>(|conn| {
            conn.batch_execute("PRAGMA journal_mode = WAL;")?;
            conn.run_pending_migrations(MIGRATIONS)
                .map_err(diesel::result::Error::QueryBuilderError)?;

            let sqlite_version =
                sql_query("SELECT sqlite_version() AS version").load::<SqliteVersion>(conn)?;
            tracing::info!("sqlite_version={}", sqlite_version[0].version);

            tracing::info!("Migrations successful");
            Ok(())
        })?;

        Ok::<_, StorageError>(())
    }

    pub fn mls_provider(&self) -> Result<XmtpOpenMlsProvider<Db::Connection>, StorageError> {
        let conn = self.conn()?;
        Ok(XmtpOpenMlsProvider::new(conn))
    }

    /// Access to the database queries defined on connections
    pub fn db(&self) -> DbConnection<Db::Connection> {
        DbConnection::new(self.db.conn())
    }

    /// Pulls a new connection from the store
    pub fn conn(&self) -> Result<Db::Connection, StorageError> {
        Ok(self.db.conn())
    }

    /// Release connection to the database, closing it
    pub fn release_connection(&self) -> Result<(), StorageError> {
        Ok(self.disconnect()?)
    }

    /// Reconnect to the database
    pub fn reconnect(&self) -> Result<(), StorageError> {
        Ok(self.db.reconnect()?)
    }
}

impl<Db> XmtpDb for EncryptedMessageStore<Db>
where
    Db: XmtpDb,
{
    type Error = Db::Error;

    type Connection = Db::Connection;

    fn conn(&self) -> Self::Connection {
        self.db.conn()
    }

    fn reconnect(&self) -> Result<(), Self::Error> {
        self.db.reconnect()
    }

    fn disconnect(&self) -> Result<(), Self::Error> {
        self.db.disconnect()
    }
}
