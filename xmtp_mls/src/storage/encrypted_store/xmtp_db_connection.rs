use crate::storage::DbConnection;
use std::{cell::RefCell, fmt};

/// A wrapper for DbConnection that houses all XMTP DB operations.
/// Uses a RefCell internally for interior mutability, so that the connection
/// and transaction state can be shared between the OpenMLS Provider and
/// native XMTP operations
pub struct XmtpDbConnection<'a> {
    pub(crate) wrapped_conn: RefCell<&'a mut DbConnection>,
}

impl<'a> XmtpDbConnection<'a> {
    pub fn new(conn: &'a mut DbConnection) -> Self {
        Self {
            wrapped_conn: RefCell::new(conn),
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
