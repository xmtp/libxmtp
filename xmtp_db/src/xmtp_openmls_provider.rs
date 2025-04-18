#[cfg(not(target_arch = "wasm32"))]
use crate::native::NativeDb;
#[cfg(target_arch = "wasm32")]
use crate::wasm::WasmDb;
use crate::{db_connection::DbConnectionPrivate, sql_key_store::SqlKeyStore};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;
use std::marker::PhantomData;

#[cfg(target_arch = "wasm32")]
pub type XmtpOpenMlsProvider = XmtpOpenMlsProviderPrivate<WasmDb, crate::RawDbConnection>;
#[cfg(not(target_arch = "wasm32"))]
pub type XmtpOpenMlsProvider = XmtpOpenMlsProviderPrivate<NativeDb, crate::RawDbConnection>;

#[derive(Debug)]
pub struct XmtpOpenMlsProviderPrivate<Db, C> {
    crypto: RustCrypto,
    key_store: SqlKeyStore<C>,
    // This is here for the ProviderTransaction trait
    // to avoid having to put explicit type annotations everywhere.
    _phantom: PhantomData<Db>,
}

impl<Db, C> XmtpOpenMlsProviderPrivate<Db, C> {
    pub fn new(conn: DbConnectionPrivate<C>) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: SqlKeyStore::new(conn),
            _phantom: PhantomData,
        }
    }

    pub fn new_crypto() -> RustCrypto {
        RustCrypto::default()
    }

    pub fn conn_ref(&self) -> &DbConnectionPrivate<C> {
        self.key_store.conn_ref()
    }
}

impl<Db, C> OpenMlsProvider for XmtpOpenMlsProviderPrivate<Db, C>
where
    C: diesel::Connection<Backend = crate::Sqlite> + diesel::connection::LoadConnection,
{
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = SqlKeyStore<C>;

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }

    fn storage(&self) -> &Self::StorageProvider {
        &self.key_store
    }
}

impl<Db, C> OpenMlsProvider for &XmtpOpenMlsProviderPrivate<Db, C>
where
    C: diesel::Connection<Backend = crate::Sqlite> + diesel::connection::LoadConnection,
{
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = SqlKeyStore<C>;

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }

    fn storage(&self) -> &Self::StorageProvider {
        &self.key_store
    }
}
