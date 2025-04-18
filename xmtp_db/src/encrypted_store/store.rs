use super::*;
use crate::xmtp_openmls_provider::XmtpOpenMlsProvider;
use derive_builder::Builder;

/// Manages a Sqlite db for persisting messages and other objects.
#[derive(Clone, Debug, Builder)]
#[builder(setter(into))]
pub struct EncryptedMessageStore<Db> {
    pub(super) db: Db,
}

impl<Db> EncryptedMessageStore<Db> {
    pub fn builder() -> EncryptedMessageStoreBuilder<Db>
    where
        Db: Clone,
    {
        Default::default()
    }
}

impl<Db, E> EncryptedMessageStore<Db>
where
    Db: XmtpDb<Error = E>,
    StorageError: From<<<Db as XmtpDb>::Connection as ConnectionExt>::Error>,
    StorageError: From<E>,
{
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
