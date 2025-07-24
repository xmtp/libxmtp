use std::sync::Arc;
use xmtp_db::{ConnectionError, DbConnection, DefaultDatabase, StorageOption, XmtpDb};

pub mod chaos;

#[derive(Clone)]
pub struct ChaosDb<Db = DefaultDatabase>
where
    Db: XmtpDb,
{
    db: Db,
    conn: Arc<chaos::ChaosConnection<<Db as XmtpDb>::Connection>>,
}

impl<Db: XmtpDb> ChaosDb<Db> {
    pub fn builder(db: Db) -> ChaosDbBuilder<Db> {
        ChaosDbBuilder {
            db,
            error_frequency: 0.0,
        }
    }
}

impl<Db> XmtpDb for ChaosDb<Db>
where
    Db: XmtpDb,
    <Db as XmtpDb>::Connection: Send + Sync,
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

#[derive(Default)]
pub struct ChaosDbBuilder<Db> {
    error_frequency: f64,
    db: Db,
}

impl<Db> ChaosDbBuilder<Db> {
    pub fn error_frequency(self, f: f64) -> Self {
        Self {
            error_frequency: f,
            ..self
        }
    }

    pub fn db<NewDb>(self, db: NewDb) -> ChaosDbBuilder<NewDb> {
        ChaosDbBuilder {
            db,
            error_frequency: self.error_frequency,
        }
    }
}

impl<Db> ChaosDbBuilder<Db>
where
    Db: XmtpDb + Clone,
    <Db as XmtpDb>::Connection: Clone + Send + Sync,
{
    /// Build the EncryptedMessageStore with Chaos
    /// Returns a tuple of (ChaosConnection, EncryptedMessageStore)
    /// the ChaosConnection may be used to add cHaOS
    pub fn build(
        self,
    ) -> (
        Arc<chaos::ChaosConnection<<Db as XmtpDb>::Connection>>,
        xmtp_db::store::EncryptedMessageStore<ChaosDb<Db>>,
    ) {
        self.db.init(self.db.opts()).unwrap();
        let conn = chaos::ChaosConnection::builder()
            .db(self.db.conn())
            // if we dont set frequency here, the chaos builder itself might fail building
            .error_frequency(self.error_frequency)
            .build()
            .unwrap();
        let conn = Arc::new(conn);
        let chaos_db = ChaosDb::<Db> {
            db: self.db,
            conn: conn.clone(),
        };
        let store = xmtp_db::store::EncryptedMessageStore::<ChaosDb<Db>>::builder()
            .db(chaos_db)
            .build()
            .unwrap();
        (conn, store)
    }
}
