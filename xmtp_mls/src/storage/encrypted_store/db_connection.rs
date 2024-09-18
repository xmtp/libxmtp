use parking_lot::Mutex;
use std::fmt;
use std::sync::Arc;
use crate::xmtp_openmls_provider::XmtpOpenMlsProvider;

#[cfg(not(target_arch = "wasm32"))]
pub type DbConnection = DbConnectionPrivate<super::RawDbConnection>;

#[cfg(target_arch = "wasm32")]
pub type DbConnection = DbConnectionPrivate<diesel_wasm_sqlite::connection::WasmSqliteConnection>;

/// A wrapper for RawDbConnection that houses all XMTP DB operations.
/// Uses a [`Mutex]` internally for interior mutability, so that the connection
/// and transaction state can be shared between the OpenMLS Provider and
/// native XMTP operations
// ~~~~ _NOTE_ ~~~~~
// Do not derive clone here.
// callers should be able to accomplish everything with one conn/reference.
#[doc(hidden)]
pub struct DbConnectionPrivate<C> {
    wrapped_conn: Arc<Mutex<C>>,
}

/// Owned DBConnection Methods
/// Lifetime is 'static' because we are using [`RefOrValue::Value`] variant.
impl<C> DbConnectionPrivate<C> {
    pub(super) fn new(conn: C) -> Self {
        Self {
            wrapped_conn: Arc::new(Mutex::new(conn)),
        }
    }

    pub(super) fn from_arc_mutex(conn: Arc<Mutex<C>>) -> Self {
        Self { wrapped_conn: conn }
    }
}

impl<C> DbConnectionPrivate<C>
where
    C: diesel::Connection,
{
    pub(crate) fn raw_query<T, E, F>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut C) -> Result<T, E>,
    {
        let mut lock = self.wrapped_conn.lock();
        fun(&mut lock)
    }
}

// Forces a move for conn
// This is an important distinction from deriving `Clone` on `DbConnection`.
// This way, conn will be moved into XmtpOpenMlsProvider. This forces codepaths to
// use a connection from the provider, rather than pulling a new one from the pool, resulting
// in two connections in the same scope.
impl From<DbConnection> for XmtpOpenMlsProvider {
    fn from(conn: DbConnection) -> XmtpOpenMlsProvider {
        XmtpOpenMlsProvider::new(conn)
    }
}

impl<C> fmt::Debug for DbConnectionPrivate<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbConnection")
            .field("wrapped_conn", &"DbConnection")
            .finish()
    }
}

