//! Native SQLite faults shared by every store and connection in one process.

use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use rand::{RngExt, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use xmtp_common::time::{Duration, Instant};
use xmtp_db::{
    ConnectionError, ConnectionExt, DbConnection, EncryptedMessageStore, StorageOption, XmtpDb,
    database::{NativeDb, PlatformStorageError},
    diesel::{SqliteConnection, result::DatabaseErrorKind},
};

const PERCENT: u8 = 100;

pub type ChaosStore = EncryptedMessageStore<FaultDb>;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskFault {
    Locked,
    Io,
    Full,
    AfterCall,
}

impl DiskFault {
    fn error(self) -> ConnectionError {
        // SQLite maps these errors to Diesel's Unknown database error kind.
        let message = match self {
            Self::Locked => "database is locked",
            Self::Io | Self::AfterCall => "disk I/O error",
            Self::Full => "database or disk is full",
        };
        xmtp_db::diesel::result::Error::DatabaseError(
            DatabaseErrorKind::Unknown,
            Box::new(message.to_owned()),
        )
        .into()
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct DiskStats {
    pub calls: u64,
    pub injected_before: u64,
    pub maybe_committed: u64,
}

struct Window {
    fault: DiskFault,
    until: Instant,
    probability_percent: u8,
}

struct State {
    rng: StdRng,
    window: Option<Window>,
    disconnected_until: Option<Instant>,
    stats: DiskStats,
}

struct Shared {
    connection: <NativeDb as XmtpDb>::Connection,
    // Clearing a fault must wait for calls that already selected that fault.
    lifecycle: RwLock<()>,
    state: Mutex<State>,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn expire_connection_window(&self, state: &mut State) -> Result<(), ConnectionError> {
        if let Some(until) = state.disconnected_until {
            if Instant::now() < until {
                return Err(PlatformStorageError::PoolNeedsConnection.into());
            }
            self.connection.reconnect()?;
            state.disconnected_until = None;
        }
        Ok(())
    }
}

/// Control belongs to the child process and affects its existing SDK handles.
#[derive(Clone)]
pub struct DiskControl(Arc<Shared>);

impl DiskControl {
    pub fn arm(
        &self,
        fault: DiskFault,
        duration: Duration,
        probability_percent: u8,
    ) -> Result<(), ConnectionError> {
        if probability_percent > PERCENT {
            return Err(ConnectionError::InvalidQuery(
                "disk fault probability must be at most 100 percent".into(),
            ));
        }
        let _gate = self.0.lifecycle.write().unwrap_or_else(|e| e.into_inner());
        let mut state = self.0.state();
        if state
            .window
            .as_ref()
            .is_some_and(|w| Instant::now() < w.until)
        {
            return Err(ConnectionError::InvalidQuery(
                "a disk fault window is already active".into(),
            ));
        }
        state.window = Some(Window {
            fault,
            until: Instant::now() + duration,
            probability_percent,
        });
        Ok(())
    }

    /// Release the native connection. SDK reconnect attempts cannot end this window.
    pub fn disconnect_for(&self, duration: Duration) -> Result<(), ConnectionError> {
        let _gate = self.0.lifecycle.write().unwrap_or_else(|e| e.into_inner());
        let mut state = self.0.state();
        if state
            .disconnected_until
            .is_some_and(|until| Instant::now() < until)
        {
            return Err(ConnectionError::InvalidQuery(
                "a connection fault window is already active".into(),
            ));
        }
        self.0.connection.disconnect()?;
        state.disconnected_until = Some(Instant::now() + duration);
        Ok(())
    }

    pub fn clear(&self) -> Result<(), ConnectionError> {
        let _gate = self.0.lifecycle.write().unwrap_or_else(|e| e.into_inner());
        let mut state = self.0.state();
        state.window = None;
        if state.disconnected_until.is_some() {
            self.0.connection.reconnect()?;
            state.disconnected_until = None;
        }
        Ok(())
    }

    pub fn stats(&self) -> DiskStats {
        self.0.state().stats
    }

    pub fn maybe_committed(&self) -> u64 {
        self.stats().maybe_committed
    }
}

#[derive(Clone)]
pub struct FaultConnection(DiskControl);

impl ConnectionExt for FaultConnection {
    fn raw_query<T, F>(&self, fun: F) -> Result<T, ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, xmtp_db::diesel::result::Error>,
    {
        let shared = &self.0.0;
        let _gate = shared.lifecycle.read().unwrap_or_else(|e| e.into_inner());
        let fault = {
            let mut state = shared.state();
            state.stats.calls += 1;
            shared.expire_connection_window(&mut state)?;
            if state
                .window
                .as_ref()
                .is_some_and(|w| Instant::now() >= w.until)
            {
                state.window = None;
            }
            let selected = state
                .window
                .as_ref()
                .map(|w| (w.fault, w.probability_percent));
            selected.and_then(|(fault, percent)| {
                state
                    .rng
                    .random_ratio(u32::from(percent), u32::from(PERCENT))
                    .then_some(fault)
            })
        };
        if let Some(fault @ (DiskFault::Locked | DiskFault::Io | DiskFault::Full)) = fault {
            shared.state().stats.injected_before += 1;
            return Err(fault.error());
        }
        let result = shared.connection.raw_query(fun);
        if matches!(fault, Some(DiskFault::AfterCall)) && result.is_ok() {
            // A successful outer call can contain an inner transaction error.
            // This counter says only that the call ran before its result was lost.
            let maybe_committed = {
                let mut state = shared.state();
                state.stats.maybe_committed += 1;
                state.stats.maybe_committed
            };
            tracing::debug!(maybe_committed, "database call failed after execution");
            return Err(DiskFault::AfterCall.error());
        }
        result
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        let shared = &self.0.0;
        let _gate = shared.lifecycle.write().unwrap_or_else(|e| e.into_inner());
        shared.connection.disconnect()
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        let shared = &self.0.0;
        let _gate = shared.lifecycle.write().unwrap_or_else(|e| e.into_inner());
        let mut state = shared.state();
        if state.disconnected_until.is_some() {
            shared.expire_connection_window(&mut state)
        } else {
            shared.connection.reconnect()
        }
    }
}

#[derive(Clone)]
pub struct FaultDb {
    native: NativeDb,
    control: DiskControl,
}

impl FaultDb {
    pub fn new(native: NativeDb, seed: u64) -> Self {
        let control = DiskControl(Arc::new(Shared {
            connection: native.conn(),
            lifecycle: RwLock::new(()),
            state: Mutex::new(State {
                rng: StdRng::seed_from_u64(seed),
                window: None,
                disconnected_until: None,
                stats: DiskStats::default(),
            }),
        }));
        Self { native, control }
    }

    pub fn control(&self) -> DiskControl {
        self.control.clone()
    }
}

impl XmtpDb for FaultDb {
    type Connection = FaultConnection;
    type DbQuery = DbConnection<FaultConnection>;

    fn conn(&self) -> Self::Connection {
        FaultConnection(self.control.clone())
    }

    fn db(&self) -> Self::DbQuery {
        DbConnection::new(self.conn())
    }

    fn opts(&self) -> &StorageOption {
        self.native.opts()
    }

    fn validate(&self, conn: &mut SqliteConnection) -> Result<(), ConnectionError> {
        self.native.validate(conn)
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        self.conn().disconnect()
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        self.conn().reconnect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_db::diesel::{self, RunQueryDsl, connection::SimpleConnection};

    const TEST_SEED: u64 = 42;
    const TEST_WINDOW: Duration = Duration::from_secs(60);

    fn fixture() -> Result<(tempfile::TempDir, ChaosStore, DiskControl), Box<dyn std::error::Error>>
    {
        let directory = tempfile::tempdir()?;
        let native = NativeDb::builder()
            .persistent(directory.path().join("fault.sqlite3").to_string_lossy())
            .single_connection()
            .build_unencrypted()?;
        let database = FaultDb::new(native, TEST_SEED);
        let control = database.control();
        let store = ChaosStore::new(database)?;
        store.conn().raw_query(|conn| {
            conn.batch_execute("CREATE TABLE fault_probe (value INTEGER NOT NULL)")
        })?;
        Ok((directory, store, control))
    }

    fn count(store: &ChaosStore) -> Result<i64, ConnectionError> {
        #[derive(diesel::QueryableByName)]
        struct Count {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            count: i64,
        }
        store.conn().raw_query(|conn| {
            diesel::sql_query("SELECT COUNT(*) AS count FROM fault_probe")
                .get_result::<Count>(conn)
                .map(|row| row.count)
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn pre_call_faults_do_not_execute_sql() {
        let (_directory, store, control) = fixture()?;
        for fault in [DiskFault::Locked, DiskFault::Io, DiskFault::Full] {
            control.arm(fault, TEST_WINDOW, PERCENT)?;
            let error = store
                .db()
                .raw_query(|conn| conn.batch_execute("INSERT INTO fault_probe VALUES (1)"))
                .unwrap_err();
            assert_eq!(error.to_string(), fault.error().to_string());
            control.clear()?;
            assert_eq!(count(&store)?, 0);
        }
        assert_eq!(control.stats().injected_before, 3);
        assert_eq!(control.maybe_committed(), 0);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn post_call_error_preserves_committed_write_and_records_uncertainty() {
        let (_directory, store, control) = fixture()?;
        control.arm(DiskFault::AfterCall, TEST_WINDOW, PERCENT)?;
        let result = store.conn().raw_query(|conn| {
            conn.immediate_transaction(|conn| {
                conn.batch_execute("INSERT INTO fault_probe VALUES (1)")
            })
        });
        assert!(result.is_err());
        assert_eq!(control.maybe_committed(), 1);
        // An error from SQLite itself is retained and is not a lost success.
        assert!(
            store
                .conn()
                .raw_query(|conn| conn.batch_execute("INVALID SQL"))
                .is_err()
        );
        assert_eq!(control.maybe_committed(), 1);
        control.clear()?;
        assert_eq!(count(&store)?, 1);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn connection_window_blocks_existing_handles_and_reconnect_then_recovers() {
        let (_directory, store, control) = fixture()?;
        let existing = store.db();
        existing.raw_query(|conn| conn.batch_execute("INSERT INTO fault_probe VALUES (1)"))?;
        control.disconnect_for(TEST_WINDOW)?;
        assert!(
            existing
                .raw_query(|_| Ok(()))
                .unwrap_err()
                .db_needs_connection()
        );
        assert!(store.reconnect().unwrap_err().db_needs_connection());
        control.clear()?;
        existing.raw_query(|conn| conn.batch_execute("INSERT INTO fault_probe VALUES (2)"))?;
        assert_eq!(count(&store)?, 2);
        control.disconnect_for(Duration::ZERO)?;
        assert_eq!(count(&store)?, 2);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn expired_or_zero_probability_window_permits_sql() {
        let (_directory, store, control) = fixture()?;
        control.arm(DiskFault::Full, Duration::ZERO, PERCENT)?;
        assert_eq!(count(&store)?, 0);
        control.arm(DiskFault::Full, TEST_WINDOW, 0)?;
        assert_eq!(count(&store)?, 0);
        assert_eq!(control.stats().injected_before, 0);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn seed_controls_fault_probability() {
        const CALLS: usize = 64;
        let (_first_directory, first, first_control) = fixture()?;
        let (_second_directory, second, second_control) = fixture()?;
        first_control.arm(DiskFault::Locked, TEST_WINDOW, PERCENT / 2)?;
        second_control.arm(DiskFault::Locked, TEST_WINDOW, PERCENT / 2)?;
        let first_results: Vec<_> = (0..CALLS).map(|_| count(&first).is_err()).collect();
        let second_results: Vec<_> = (0..CALLS).map(|_| count(&second).is_err()).collect();
        assert_eq!(first_results, second_results);
        assert!(first_results.iter().any(|failed| *failed));
        assert!(first_results.iter().any(|failed| !failed));
    }
}
