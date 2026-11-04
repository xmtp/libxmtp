//! XMTP DB Pool

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use bon::bon;
use diesel::{
    RunQueryDsl, SqliteConnection,
    r2d2::{self, ManageConnection, PooledConnection},
};
use parking_lot::{Mutex, RwLock};

use crate::{PlatformStorageError, StorageOption, native::XmtpConnection};
use xmtp_configuration::{MAX_DB_POOL_SIZE, MIN_DB_POOL_SIZE};
type Pool = r2d2::Pool<ConnectionManager>;

/// The least time between two WAL truncates of a pool's database. A truncate
/// clears deleted content the WAL still holds, so this bounds how long it stays
/// there, without stalling writers each time the pool adds or hands out a
/// connection.
const WAL_TRUNCATE_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// When a truncate failed or a reader blocked it, the delay before the next
/// try. A blocked try can wait the whole busy timeout, so not on every checkout.
const WAL_TRUNCATE_RETRY: Duration = Duration::from_secs(60);

/// When a pool next truncates its WAL, shared by its new connections and its
/// checkouts. It starts due, so the pool's first connection truncates the WAL
/// that the last run left.
#[derive(Clone)]
struct WalTruncate {
    next: Arc<Mutex<Instant>>,
}

impl WalTruncate {
    fn new() -> Self {
        Self {
            next: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Truncate the WAL on `connection` once the interval since the last
    /// truncate has passed. Only a completed truncate moves the next one an
    /// interval on; a failed or blocked one is tried again after a short delay.
    fn truncate_if_due(&self, connection: &mut SqliteConnection) -> diesel::QueryResult<()> {
        let now = Instant::now();
        {
            let mut next = self.next.lock();
            if now < *next {
                return Ok(());
            }
            // Claim this try without holding the lock while it runs.
            *next = now + WAL_TRUNCATE_RETRY;
        }
        let checkpoint = diesel::sql_query("PRAGMA wal_checkpoint(TRUNCATE)")
            .get_result::<Checkpoint>(connection)?;
        if checkpoint.busy == 0 {
            *self.next.lock() = Instant::now() + WAL_TRUNCATE_INTERVAL;
        }
        Ok(())
    }
}

/// The result row of `PRAGMA wal_checkpoint`; `busy` is 1 when a reader or
/// writer blocked the checkpoint.
#[derive(diesel::QueryableByName)]
struct Checkpoint {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    busy: i32,
}

pub struct ConnectionManager {
    inner: r2d2::ConnectionManager<SqliteConnection>,
    customizer: Box<dyn XmtpConnection>,
    open: Arc<RwLock<bool>>,
    wal_truncate: WalTruncate,
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
        // The pool adds connections while idle too, as it replaces old ones.
        self.wal_truncate.truncate_if_due(&mut connection)?;
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
    wal_truncate: WalTruncate,
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
        let wal_truncate = WalTruncate::new();
        let manager = ConnectionManager {
            inner: r2d2::ConnectionManager::new(path),
            customizer: customizer.clone(),
            open: open.clone(),
            wal_truncate: wal_truncate.clone(),
        };
        let pool = Pool::builder()
            .max_size(max_size.unwrap_or(MAX_DB_POOL_SIZE))
            .min_idle(Some(min_size.unwrap_or(MIN_DB_POOL_SIZE)))
            .build(manager)?;

        super::database_open_pragmas(&mut *pool.get()?)?;
        Ok(Self {
            inner: pool,
            open,
            wal_truncate,
        })
    }

    pub fn close(&self) {
        *self.open.write() = false;
    }

    pub fn get(&self) -> Result<PooledConnection<ConnectionManager>, PlatformStorageError> {
        let mut connection = self.inner.get()?;
        // A busy pool may add no connection for long; its checkouts keep the bound.
        self.wal_truncate.truncate_if_due(&mut connection)?;
        Ok(connection)
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
        ValidatedConnection, prelude::*,
    };

    use super::*;
    use diesel::{connection::SimpleConnection, prelude::*};
    use rstest::*;
    use xmtp_configuration::BUSY_TIMEOUT;

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
            wal_truncate: pool.wal_truncate.clone(),
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
        assert_eq!(crate::DbConnection::new(pool).busy_timeout()?, BUSY_TIMEOUT);
        Ok(())
    }

    fn unencrypted(path: &str) -> Box<dyn XmtpConnection> {
        Box::new(UnencryptedConnection::new(StorageOption::Persistent(
            path.into(),
        )))
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn opening_truncates_leftover_wal() {
        let path = xmtp_common::tmp_path();
        let earlier = DbPool::builder().customizer(unencrypted(&path)).build()?;
        // An open connection keeps SQLite from removing the WAL.
        let mut connection = earlier.get()?;
        connection.batch_execute("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);")?;
        let wal = format!("{path}-wal");
        assert!(std::fs::metadata(&wal)?.len() > 0);

        let _reopened = DbPool::builder().customizer(unencrypted(&path)).build()?;
        assert_eq!(
            std::fs::metadata(&wal)?.len(),
            0,
            "opening left the earlier WAL in place"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn a_single_connection_truncates_leftover_wal_on_open_and_reconnect() {
        let path = xmtp_common::tmp_path();
        let earlier = DbPool::builder().customizer(unencrypted(&path)).build()?;
        // An open connection keeps SQLite from removing the WAL.
        let mut connection = earlier.get()?;
        connection.batch_execute("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);")?;
        let wal = format!("{path}-wal");
        let single = super::super::SingleDbConnection::new(unencrypted(&path))?;
        assert_eq!(std::fs::metadata(&wal)?.len(), 0, "opening left the WAL");

        connection.batch_execute("INSERT INTO t VALUES (2);")?;
        assert!(std::fs::metadata(&wal)?.len() > 0);
        single.db_reconnect()?;
        assert_eq!(
            std::fs::metadata(&wal)?.len(),
            0,
            "reconnecting left the WAL"
        );
    }

    /// Reports a locked database at once instead of after the busy timeout.
    #[derive(Clone, Debug)]
    struct NoBusyWait(UnencryptedConnection);

    impl super::super::ConnectionOptions for NoBusyWait {
        fn options(&self) -> &StorageOption {
            self.0.options()
        }
    }

    impl ValidatedConnection for NoBusyWait {}

    impl r2d2::CustomizeConnection<SqliteConnection, r2d2::Error> for NoBusyWait {
        fn on_acquire(&self, connection: &mut SqliteConnection) -> Result<(), r2d2::Error> {
            self.0.on_acquire(connection)?;
            connection.batch_execute("PRAGMA busy_timeout = 0;")?;
            Ok(())
        }
    }

    /// A reader that blocks the truncate when the pool opens delays it by the
    /// short retry, not a full interval.
    #[xmtp_common::test(unwrap_try = true)]
    fn a_blocked_open_truncate_is_retried_after_a_short_delay() {
        let path = xmtp_common::tmp_path();
        let other = DbPool::builder()
            .customizer(unencrypted(&path))
            .max_size(2)
            .min_size(0)
            .build()?;
        let mut writer = other.get()?;
        writer.batch_execute("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);")?;
        let mut reader = other.get()?;
        reader.batch_execute("BEGIN; SELECT * FROM t;")?;
        let pool = DbPool::builder()
            .customizer(Box::new(NoBusyWait(UnencryptedConnection::new(
                StorageOption::Persistent(path.clone()),
            ))))
            .max_size(2)
            .min_size(0)
            .build()?;
        let wal = format!("{path}-wal");
        let wal_len = || std::fs::metadata(&wal).unwrap().len();
        let next_in = || {
            pool.wal_truncate
                .next
                .lock()
                .saturating_duration_since(Instant::now())
        };
        assert!(wal_len() > 0);
        assert!(next_in() > Duration::ZERO && next_in() <= WAL_TRUNCATE_RETRY);
        reader.batch_execute("COMMIT;")?;
        *pool.wal_truncate.next.lock() = Instant::now();
        drop(pool.get()?);
        assert_eq!(wal_len(), 0);
    }

    /// A truncate that a reader blocks is tried again after a short delay, not
    /// a full interval, and truncates once the reader is gone.
    #[xmtp_common::test(unwrap_try = true)]
    fn a_blocked_truncate_is_retried_after_a_short_delay() {
        let path = xmtp_common::tmp_path();
        let pool = DbPool::builder()
            .customizer(unencrypted(&path))
            .max_size(3)
            .min_size(0)
            .build()?;
        let wal = format!("{path}-wal");
        let wal_len = || std::fs::metadata(&wal).unwrap().len();
        let mut writer = pool.get()?;
        writer.batch_execute("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);")?;
        let mut reader = pool.get()?;
        reader.batch_execute("BEGIN; SELECT * FROM t;")?;
        // Report the reader at once instead of after the busy timeout.
        let mut connection = pool.get()?;
        connection.batch_execute("PRAGMA busy_timeout = 0;")?;
        let gate = &pool.wal_truncate;
        let next_in = || gate.next.lock().saturating_duration_since(Instant::now());
        *gate.next.lock() = Instant::now();
        gate.truncate_if_due(&mut connection)?;
        assert!(wal_len() > 0);
        assert!(next_in() > Duration::ZERO && next_in() <= WAL_TRUNCATE_RETRY);
        reader.batch_execute("COMMIT;")?;
        *gate.next.lock() = Instant::now();
        gate.truncate_if_due(&mut connection)?;
        assert_eq!(wal_len(), 0);
        assert!(next_in() > WAL_TRUNCATE_RETRY);
    }

    /// A connection the pool hands out or adds truncates the WAL only once the
    /// interval since the last truncate has passed; opening the database counts
    /// as one.
    #[xmtp_common::test(unwrap_try = true)]
    fn the_pool_truncates_the_wal_only_after_the_interval() {
        let path = xmtp_common::tmp_path();
        let pool = DbPool::builder()
            .customizer(unencrypted(&path))
            .max_size(2)
            .min_size(0)
            .build()?;
        let wal = format!("{path}-wal");
        let wal_len = || std::fs::metadata(&wal).unwrap().len();
        let mut writer = pool.get()?;
        writer.batch_execute("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);")?;
        // Before the interval: an added connection, then a checkout of it.
        drop(pool.get()?);
        drop(pool.get()?);
        assert!(wal_len() > 0, "a connection truncated before the interval");
        *pool.wal_truncate.next.lock() = Instant::now();
        drop(pool.get()?);
        assert_eq!(wal_len(), 0, "a checkout after the interval left the WAL");
        // A connection the pool adds while idle, as when it replaces an old one.
        writer.batch_execute("INSERT INTO t VALUES (2);")?;
        *pool.wal_truncate.next.lock() = Instant::now();
        let manager = ConnectionManager {
            inner: r2d2::ConnectionManager::new(path.clone()),
            customizer: unencrypted(&path),
            open: pool.open.clone(),
            wal_truncate: pool.wal_truncate.clone(),
        };
        manager.connect()?;
        assert_eq!(
            wal_len(),
            0,
            "a new connection after the interval left the WAL"
        );
    }
}
