//! XMTP DB Pool

use diesel::{
    SqliteConnection,
    connection::SimpleConnection,
    r2d2::{self, PooledConnection},
};

use crate::{PlatformStorageError, StorageOption, native::XmtpConnection};
use xmtp_configuration::{BUSY_TIMEOUT, MAX_DB_POOL_SIZE, MIN_DB_POOL_SIZE};
type Pool = r2d2::Pool<ConnectionManager>;
pub type ConnectionManager = r2d2::ConnectionManager<SqliteConnection>;

pub struct DbPool {
    inner: Pool,
}

impl DbPool {
    pub(super) fn new(customizer: Box<dyn XmtpConnection>) -> Result<Self, PlatformStorageError> {
        let StorageOption::Persistent(path) = customizer.options() else {
            return Err(PlatformStorageError::PoolRequiresPath);
        };
        let pool = Pool::builder()
            .connection_customizer(customizer.clone())
            .max_size(MAX_DB_POOL_SIZE)
            .min_idle(Some(MIN_DB_POOL_SIZE))
            .build(ConnectionManager::new(path))?;

        let mut c = pool.get()?;
        c.batch_execute(&format!("PRAGMA busy_timeout = {};", BUSY_TIMEOUT))?;
        c.batch_execute("PRAGMA journal_mode = WAL;")?;
        Ok(Self { inner: pool })
    }

    pub fn get(&self) -> Result<PooledConnection<ConnectionManager>, PlatformStorageError> {
        self.inner.get().map_err(Into::into)
    }

    pub fn state(&self) -> r2d2::State {
        self.inner.state()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ConnectionExt, EncryptedConnection, StorageOption, UnencryptedConnection,
        ValidatedConnection, prelude::*,
    };

    use super::*;
    use diesel::prelude::*;
    use rstest::*;

    impl ConnectionExt for DbPool {
        fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
        where
            F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
            Self: Sized,
        {
            let mut c = self.get().unwrap();
            Ok(fun(&mut c)?)
        }

        fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
        where
            F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
            Self: Sized,
        {
            let mut c = self.get().unwrap();
            Ok(fun(&mut c)?)
        }

        fn disconnect(&self) -> Result<(), crate::ConnectionError> {
            todo!()
        }

        fn reconnect(&self) -> Result<(), crate::ConnectionError> {
            todo!()
        }
    }

    #[derive(QueryableByName, Debug)]
    struct JournalMode {
        #[diesel(sql_type = diesel::sql_types::Text)]
        journal_mode: String,
    }

    #[derive(QueryableByName, Debug)]
    struct Synchronous {
        #[diesel(sql_type = diesel::sql_types::Integer)]
        synchronous: i32,
    }

    #[derive(QueryableByName, Debug)]
    struct Autocheckpoint {
        #[diesel(sql_type = diesel::sql_types::Integer)]
        wal_autocheckpoint: i32,
    }

    #[fixture]
    fn encrypted_connection() -> Box<dyn XmtpConnection> {
        let path = xmtp_common::tmp_path();
        let c = Box::new(
            EncryptedConnection::new([0u8; 32], &StorageOption::Persistent(path.clone())).unwrap(),
        );
        let mut conn = SqliteConnection::establish(&path).unwrap();
        // do simple db queries to ensure encrypted database sets up correctly
        c.validate(&mut conn).unwrap();
        c
    }

    #[fixture]
    fn unencrypted_connection() -> Box<dyn XmtpConnection> {
        let path = xmtp_common::tmp_path();
        Box::new(UnencryptedConnection::new(StorageOption::Persistent(path)))
    }

    #[rstest]
    #[case(encrypted_connection())]
    #[case(unencrypted_connection())]
    #[test]
    pub fn sets_busy_timeout(#[case] customizer: Box<dyn XmtpConnection>) {
        use crate::DbConnection;
        use xmtp_configuration::BUSY_TIMEOUT;
        let pool = DbPool::new(customizer.clone()).unwrap();
        let dbconn = DbConnection::new(pool);
        let timeout = dbconn.busy_timeout().unwrap();
        assert_eq!(timeout, BUSY_TIMEOUT, "wrong timeout");
    }

    #[rstest]
    #[case(encrypted_connection())]
    #[case(unencrypted_connection())]
    #[test]
    pub fn sets_journal_mode(#[case] customizer: Box<dyn XmtpConnection>) {
        let pool = DbPool::new(customizer.clone()).unwrap();
        let mut conn = pool.get().unwrap();
        let JournalMode { journal_mode } = diesel::sql_query("PRAGMA journal_mode")
            .get_result::<JournalMode>(&mut conn)
            .unwrap();
        assert_eq!(journal_mode, "wal", "wrong journal mode");
    }

    #[rstest]
    #[case(encrypted_connection())]
    #[case(unencrypted_connection())]
    #[test]
    pub fn sets_synchronous(#[case] customizer: Box<dyn XmtpConnection>) {
        let pool = DbPool::new(customizer.clone()).unwrap();
        let mut conn = pool.get().unwrap();
        let Synchronous { synchronous } = diesel::sql_query("PRAGMA synchronous")
            .get_result::<Synchronous>(&mut conn)
            .unwrap();
        // 1 corresponds to NORMAL
        assert_eq!(synchronous, 1, "wrong synchronous");
    }

    #[rstest]
    #[case(encrypted_connection())]
    #[case(unencrypted_connection())]
    #[test]
    pub fn sets_autocheckpoint(#[case] customizer: Box<dyn XmtpConnection>) {
        let pool = DbPool::new(customizer.clone()).unwrap();
        let mut conn = pool.get().unwrap();
        let Autocheckpoint { wal_autocheckpoint } = diesel::sql_query("PRAGMA wal_autocheckpoint")
            .get_result::<Autocheckpoint>(&mut conn)
            .unwrap();
        // 1 corresponds to NORMAL
        assert_eq!(wal_autocheckpoint, 1_000, "wrong synchronous");
    }
}
