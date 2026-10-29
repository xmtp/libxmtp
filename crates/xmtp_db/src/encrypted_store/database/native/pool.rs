//! XMTP DB Pool

use std::sync::Arc;

use bon::bon;
use diesel::{
    SqliteConnection,
    connection::SimpleConnection,
    r2d2::{self, ManageConnection, PooledConnection},
};
use parking_lot::RwLock;

use crate::{PlatformStorageError, StorageOption, native::XmtpConnection};
use xmtp_configuration::{BUSY_TIMEOUT, MAX_DB_POOL_SIZE, MIN_DB_POOL_SIZE};
type Pool = r2d2::Pool<ConnectionManager>;
pub struct ConnectionManager {
    inner: r2d2::ConnectionManager<SqliteConnection>,
    customizer: Box<dyn XmtpConnection>,
    open: Arc<RwLock<bool>>,
}

impl std::fmt::Debug for ConnectionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionManager").finish_non_exhaustive()
    }
}

impl ManageConnection for ConnectionManager {
    type Connection = SqliteConnection;
    type Error = r2d2::Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        // r2d2 can retain its shared pool while a background open is running.
        // Fence the entire open, including pragmas, before file deletion.
        let open = self.open.read();
        if !*open {
            return Err(r2d2::Error::ConnectionError(
                diesel::ConnectionError::BadConnection("database pool is closed".into()),
            ));
        }
        let mut connection = self.inner.connect()?;
        self.customizer.on_acquire(&mut connection)?;
        Ok(connection)
    }

    fn is_valid(&self, connection: &mut Self::Connection) -> Result<(), Self::Error> {
        self.inner.is_valid(connection)
    }

    fn has_broken(&self, connection: &mut Self::Connection) -> bool {
        self.inner.has_broken(connection)
    }
}

pub struct DbPool {
    inner: Pool,
    open: Arc<RwLock<bool>>,
}

#[bon]
impl DbPool {
    #[builder]
    pub(super) fn new(
        customizer: Box<dyn XmtpConnection>,
        max_size: Option<u32>,
        min_size: Option<u32>,
    ) -> Result<Self, PlatformStorageError> {
        let StorageOption::Persistent(path) = customizer.options() else {
            return Err(PlatformStorageError::PoolRequiresPath);
        };
        let open = Arc::new(RwLock::new(true));
        let manager = ConnectionManager {
            inner: r2d2::ConnectionManager::new(path),
            customizer: customizer.clone(),
            open: open.clone(),
        };
        let pool = Pool::builder()
            .max_size(max_size.unwrap_or(MAX_DB_POOL_SIZE))
            .min_idle(Some(min_size.unwrap_or(MIN_DB_POOL_SIZE)))
            .build(manager)?;

        let mut c = pool.get()?;
        c.batch_execute(&format!("PRAGMA busy_timeout = {};", BUSY_TIMEOUT))?;
        c.batch_execute("PRAGMA journal_mode = WAL;")?;
        Ok(Self { inner: pool, open })
    }

    pub fn close(&self) {
        *self.open.write() = false;
    }

    pub fn get(&self) -> Result<PooledConnection<ConnectionManager>, PlatformStorageError> {
        self.inner.get().map_err(Into::into)
    }

    pub fn state(&self) -> r2d2::State {
        self.inner.state()
    }
}

impl Drop for DbPool {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ConnectionExt, EncryptedConnection, StorageOption, UnencryptedConnection,
        ValidatedConnection,
    };

    use super::*;
    use diesel::prelude::*;
    use rstest::*;

    impl ConnectionExt for DbPool {
        fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
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

    #[derive(Clone, Debug)]
    struct PausedConnection {
        inner: UnencryptedConnection,
        pause: Arc<std::sync::atomic::AtomicBool>,
        entered: Arc<std::sync::Barrier>,
        release: Arc<std::sync::Barrier>,
    }

    impl super::super::ConnectionOptions for PausedConnection {
        fn options(&self) -> &StorageOption {
            self.inner.options()
        }
    }

    impl ValidatedConnection for PausedConnection {}

    impl r2d2::CustomizeConnection<SqliteConnection, r2d2::Error> for PausedConnection {
        fn on_acquire(&self, connection: &mut SqliteConnection) -> Result<(), r2d2::Error> {
            if self.pause.swap(false, std::sync::atomic::Ordering::SeqCst) {
                self.entered.wait();
                self.release.wait();
            }
            self.inner.on_acquire(connection)
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn close_waits_for_background_opens_and_prevents_file_recreation() {
        use std::sync::{
            Barrier,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        use xmtp_common::time::Duration;

        const CLOSE_OBSERVATION_WINDOW: Duration = Duration::from_millis(100);

        let path = std::path::PathBuf::from(xmtp_common::tmp_path());
        let pause = Arc::new(AtomicBool::new(false));
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let pool = Arc::new(
            DbPool::builder()
                .customizer(Box::new(PausedConnection {
                    inner: UnencryptedConnection::new(StorageOption::Persistent(
                        path.to_string_lossy().into(),
                    )),
                    pause: pause.clone(),
                    entered: entered.clone(),
                    release: release.clone(),
                }))
                .max_size(2)
                .min_size(0)
                .build()?,
        );
        let first = pool.get()?;
        pause.store(true, Ordering::SeqCst);
        let opening = std::thread::spawn({
            let pool = pool.clone();
            move || pool.get()
        });
        entered.wait();
        let (finished, closed) = mpsc::channel();
        let closing = std::thread::spawn({
            let pool = pool.clone();
            move || {
                pool.close();
                finished.send(()).unwrap();
            }
        });
        let early_close = closed.recv_timeout(CLOSE_OBSERVATION_WINDOW);
        // Always release the initializer, including when the regression is present.
        release.wait();
        closing.join().unwrap();
        drop(opening.join().unwrap()?);
        drop(first);
        assert!(
            matches!(early_close, Err(mpsc::RecvTimeoutError::Timeout)),
            "close returned while a background connection was still initializing"
        );
        let manager = ConnectionManager {
            inner: r2d2::ConnectionManager::new(path.to_string_lossy()),
            customizer: Box::new(UnencryptedConnection::new(StorageOption::Persistent(
                path.to_string_lossy().into(),
            ))),
            open: pool.open.clone(),
        };
        drop(pool);
        std::fs::remove_file(&path)?;
        assert!(manager.connect().is_err());
        assert!(
            !path.exists(),
            "a closed pool recreated the deleted database"
        );
    }

    #[derive(QueryableByName, Debug)]
    struct ConnectionPragmas {
        #[diesel(sql_type = diesel::sql_types::Integer)]
        timeout: i32,
        #[diesel(sql_type = diesel::sql_types::Text)]
        journal_mode: String,
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
            EncryptedConnection::new([0u8; 32].into(), &StorageOption::Persistent(path.clone()))
                .unwrap(),
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
    #[xmtp_common::test(unwrap_try = true)]
    pub fn sets_connection_pragmas(
        #[case] customizer: Box<dyn XmtpConnection>,
    ) -> Result<(), crate::StorageError> {
        let pool = DbPool::builder().customizer(customizer).build()?;
        let mut first = pool.get()?;
        let mut second = pool.get()?;
        for conn in [&mut first, &mut second] {
            let pragmas = diesel::sql_query(
                "SELECT * FROM pragma_busy_timeout, pragma_journal_mode, pragma_synchronous",
            )
            .get_result::<ConnectionPragmas>(conn)?;
            assert_eq!(pragmas.timeout, BUSY_TIMEOUT, "wrong timeout");
            assert_eq!(pragmas.journal_mode, "wal", "wrong journal mode");
            assert_eq!(pragmas.synchronous, 1, "wrong synchronous"); // NORMAL
            let checkpoint = diesel::sql_query("PRAGMA wal_autocheckpoint")
                .get_result::<Autocheckpoint>(conn)?;
            assert_eq!(checkpoint.wal_autocheckpoint, 1_000, "wrong autocheckpoint");
        }
        Ok(())
    }
}
