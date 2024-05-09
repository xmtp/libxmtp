use std::sync::Arc;
use std::{fmt, sync::Mutex};

use crate::storage::RawDbConnection;

// Re-implementation of Cow without ToOwned requirement
enum RefOrValue<'a, T> {
    Ref(&'a mut T),
    Value(T),
}

/// A wrapper for RawDbConnection that houses all XMTP DB operations.
/// Uses a [`Mutex]` internally for interior mutability, so that the connection
/// and transaction state can be shared between the OpenMLS Provider and
/// native XMTP operations
pub struct DbConnection {
    wrapped_conn: Mutex<RawDbConnection>,
}

impl DbConnection {
    pub(crate) fn new(conn: RawDbConnection) -> Self {
        Self {
            wrapped_conn: Mutex::new(conn),
        }
    }

    // Note: F is a synchronous fn. If it ever becomes async, we need to use
    // tokio::sync::mutex instead of std::sync::Mutex
    pub(crate) fn raw_query<T, F>(&self, fun: F) -> Result<T, diesel::result::Error>
    where
        F: FnOnce(&mut RawDbConnection) -> Result<T, diesel::result::Error>,
    {
        let mut lock = self.wrapped_conn.lock().unwrap_or_else(
            |err| {
                log::error!(
                    "Recovering from poisoned mutex - a thread has previously panicked holding this lock"
                );
                err.into_inner()
            },
        );
        fun(*lock)
    }
}

impl fmt::Debug for DbConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbConnection")
            .field("wrapped_conn", &"DbConnection")
            .finish()
    }
}
