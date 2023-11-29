use crate::storage::RawDbConnection;
use std::{cell::RefCell, fmt};

// Re-implementation of Cow without ToOwned requirement
enum RefOrValue<'a, T> {
    Ref(&'a mut T),
    Value(T),
}

/// A wrapper for DbConnection that houses all XMTP DB operations.
/// Uses a RefCell internally for interior mutability, so that the connection
/// and transaction state can be shared between the OpenMLS Provider and
/// native XMTP operations
pub struct DbConnection<'a> {
    wrapped_conn: RefCell<RefOrValue<'a, RawDbConnection>>,
}

impl<'a> DbConnection<'a> {
    pub(crate) fn new(conn: &'a mut RawDbConnection) -> Self {
        Self {
            wrapped_conn: RefCell::new(RefOrValue::Ref(conn)),
        }
    }
    pub(crate) fn held(conn: RawDbConnection) -> Self {
        Self {
            wrapped_conn: RefCell::new(RefOrValue::Value(conn)),
        }
    }

    pub(crate) fn raw_query<T, F>(&self, fun: F) -> Result<T, diesel::result::Error>
    where
        F: FnOnce(&mut RawDbConnection) -> Result<T, diesel::result::Error>,
    {
        match *self.wrapped_conn.borrow_mut() {
            RefOrValue::Ref(ref mut conn_ref) => fun(conn_ref),
            RefOrValue::Value(ref mut conn) => fun(conn),
        }
    }
}

impl fmt::Debug for DbConnection<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbConnection")
            .field("wrapped_conn", &"DbConnection")
            .finish()
    }
}
