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

impl<Db: XmtpDb> EncryptedMessageStore<Db> {
    pub fn new(db: Db) -> Result<Self, StorageError> {
        db.init(db.opts())?;
        Ok(Self { db })
    }

    pub fn opts(&self) -> &StorageOption {
        self.db.opts()
    }
}

impl<Db> EncryptedMessageStore<Db>
where
    Db: XmtpDb,
{
    pub fn mls_provider(&self) -> XmtpOpenMlsProvider<Db::Connection> {
        XmtpOpenMlsProvider::new(self.conn())
    }

    /// Access to the database queries defined on connections
    pub fn db(&self) -> <Db as XmtpDb>::DbQuery {
        self.db.db()
    }

    /// Pulls a new connection from the store
    pub fn conn(&self) -> Db::Connection {
        self.db.conn()
    }

    /// Release connection to the database, closing it
    pub fn release_connection(&self) -> Result<(), ConnectionError> {
        self.disconnect()
    }

    /// Reconnect to the database
    pub fn reconnect(&self) -> Result<(), ConnectionError> {
        self.db.reconnect()
    }
}

impl<Db> XmtpDb for EncryptedMessageStore<Db>
where
    Db: XmtpDb,
{
    type Connection = Db::Connection;
    type DbQuery = Db::DbQuery;

    fn conn(&self) -> Self::Connection {
        self.db.conn()
    }

    fn db(&self) -> Self::DbQuery {
        self.db.db()
    }

    fn opts(&self) -> &StorageOption {
        self.db.opts()
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        self.db.reconnect()
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        self.db.disconnect()
    }
}
