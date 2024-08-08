use parking_lot::Mutex;
use std::fmt;
use std::sync::Arc;

use crate::storage::RawDbConnection;

/// A wrapper for RawDbConnection that houses all XMTP DB operations.
/// Uses a [`Mutex]` internally for interior mutability, so that the connection
/// and transaction state can be shared between the OpenMLS Provider and
/// native XMTP operations
#[derive(Clone)]
pub struct DbConnection {
    wrapped_conn: Arc<Mutex<RawDbConnection>>,
}

/// Owned DBConnection Methods
/// Lifetime is 'static' because we are using [`RefOrValue::Value`] variant.
impl DbConnection {
    pub(crate) fn new(conn: RawDbConnection) -> Self {
        Self {
            wrapped_conn: Arc::new(Mutex::new(conn)),
        }
    }

    // Note: F is a synchronous fn. If it ever becomes async, we need to use
    // tokio::sync::mutex instead of std::sync::Mutex
    pub(crate) fn raw_query<T, F>(&self, fun: F) -> Result<T, diesel::result::Error>
    where
        F: FnOnce(&mut RawDbConnection) -> Result<T, diesel::result::Error>,
    {
        let mut lock = self.wrapped_conn.lock();
        fun(&mut lock)
    }

    // Note: F is a synchronous fn. If it ever becomes async, we need to use
    // tokio::sync::mutex instead of std::sync::Mutex
    pub(crate) async fn raw_query_async<T: Send + 'static, F>(
        &self,
        fun: F,
    ) -> Result<T, diesel::result::Error>
    where
        F: FnOnce(&mut RawDbConnection) -> Result<T, diesel::result::Error> + Send + 'static,
    {
        let conn = self.wrapped_conn.clone();
        let handle = tokio::task::spawn_blocking(move || {
            let mut lock = conn.lock();
            fun(&mut lock)
        });
        match handle.await {
            Ok(res) => res,
            Err(e) => {
                if e.is_panic() {
                    std::panic::resume_unwind(e.into_panic());
                } else {
                    unreachable!("Blocking tasks cannot be cancelled. the only way they terminate is a panic");
                }
            }
        }
    }
}

impl fmt::Debug for DbConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbConnection")
            .field("wrapped_conn", &"DbConnection")
            .finish()
    }
}
