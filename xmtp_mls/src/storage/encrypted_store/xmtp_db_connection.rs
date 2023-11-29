use crate::storage::DbConnection;
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
pub struct XmtpDbConnection<'a> {
    wrapped_conn: RefCell<RefOrValue<'a, DbConnection>>,
}

impl<'a> XmtpDbConnection<'a> {
    pub(crate) fn new(conn: &'a mut DbConnection) -> Self {
        Self {
            wrapped_conn: RefCell::new(RefOrValue::Ref(conn)),
        }
    }
    pub(crate) fn held(conn: DbConnection) -> Self {
        Self {
            wrapped_conn: RefCell::new(RefOrValue::Value(conn)),
        }
    }

    pub(crate) fn raw_query<T, F>(&self, fun: F) -> Result<T, diesel::result::Error>
    where
        F: FnOnce(&mut DbConnection) -> Result<T, diesel::result::Error>,
    {
        match *self.wrapped_conn.borrow_mut() {
            RefOrValue::Ref(ref mut conn_ref) => fun(conn_ref),
            RefOrValue::Value(ref mut conn) => fun(conn),
        }
    }
}

impl fmt::Debug for XmtpDbConnection<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XmtpDbConnection")
            .field("wrapped_conn", &"DbConnection")
            .finish()
    }
}
