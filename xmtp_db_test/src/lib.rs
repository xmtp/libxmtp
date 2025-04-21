use std::sync::Arc;
use xmtp_db::{
    ConnectionError, DbConnection, DefaultDatabase, EncryptionKey, StorageOption, XmtpDb,
};

pub mod chaos;

pub type ChaosConnection = chaos::ChaosConnection<xmtp_db::DefaultConnection>;

#[derive(Clone)]
pub struct ChaosDb<Db = DefaultDatabase>
where
    Db: XmtpDb,
{
    db: Db,
    conn: Arc<chaos::ChaosConnection<<Db as XmtpDb>::Connection>>,
}

impl<Db> XmtpDb for ChaosDb<Db>
where
    Db: XmtpDb,
    <Db as XmtpDb>::Connection: Send + Sync,
    <<Db as XmtpDb>::Connection as xmtp_db::ConnectionExt>::Error: From<xmtp_db::ConnectionError>,
{
    type Connection = Arc<chaos::ChaosConnection<Db::Connection>>;

    fn conn(&self) -> Self::Connection {
        self.conn.clone()
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        self.db.reconnect()
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        self.db.disconnect()
    }

    fn opts(&self) -> &StorageOption {
        todo!()
    }

    fn db(&self) -> xmtp_db::DbConnection<Self::Connection> {
        DbConnection::new(self.conn.clone())
    }
}

pub type EncryptedMessageStore = xmtp_db::store::EncryptedMessageStore<ChaosDb>;

pub struct ChaosStoreBuilder<Db> {
    error_frequency: f64,
    db: Db,
}

impl<Db> ChaosStoreBuilder<Db> {
    pub fn error_frequency(self, f: f64) -> Self {
        Self {
            error_frequency: f,
            ..self
        }
    }

    pub fn db<NewDb>(self, db: NewDb) -> ChaosStoreBuilder<NewDb> {
        ChaosStoreBuilder {
            db,
            error_frequency: self.error_frequency,
        }
    }
}

impl<Db> ChaosStoreBuilder<Db>
where
    Db: XmtpDb + Clone,
    <Db as XmtpDb>::Connection: Clone + Send + Sync,
    xmtp_db::ConnectionError: From<<<Db as XmtpDb>::Connection as xmtp_db::ConnectionExt>::Error>,
    <<Db as XmtpDb>::Connection as xmtp_db::ConnectionExt>::Error: From<xmtp_db::ConnectionError>,
{
    /// Build the EncryptedMessageStore with Chaos
    /// Returns a tuple of (ChaosConnection, EncryptedMessageStore)
    /// the ChaosConnection may be used to add cHaOS
    pub fn build(
        self,
        opts: StorageOption,
        _enc_key: EncryptionKey,
    ) -> (
        Arc<chaos::ChaosConnection<<Db as XmtpDb>::Connection>>,
        xmtp_db::store::EncryptedMessageStore<ChaosDb<Db>>,
    ) {
        let conn = chaos::ChaosConnection::builder()
            .db(self.db.conn())
            .error_frequency(self.error_frequency)
            .build()
            .unwrap();
        let conn = Arc::new(conn);
        let chaos_db = ChaosDb::<Db> {
            db: self.db,
            conn: conn.clone(),
        };
        chaos_db.db.init(&opts).unwrap();
        let store = xmtp_db::store::EncryptedMessageStore::<ChaosDb<Db>>::builder()
            .db(chaos_db)
            .build()
            .unwrap();
        (conn, store)
    }
}
