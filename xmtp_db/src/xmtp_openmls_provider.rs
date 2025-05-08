use crate::{ConnectionExt, DbConnection};
use crate::{MlsProviderExt, sql_key_store::SqlKeyStore};
use diesel::Connection;
use diesel::connection::TransactionManager;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

pub struct XmtpOpenMlsProvider<C = crate::DefaultConnection> {
    crypto: RustCrypto,
    key_store: SqlKeyStore<C>,
}

impl<C> XmtpOpenMlsProvider<C> {
    pub fn new(conn: C) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: SqlKeyStore::new(conn),
        }
    }
}

impl<C> XmtpOpenMlsProvider<C>
where
    C: ConnectionExt,
{
    pub fn db(&self) -> &DbConnection<C> {
        self.key_store.db()
    }

    fn inner_transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProvider<C>) -> Result<T, E>,
        E: From<crate::ConnectionError> + std::error::Error,
    {
        tracing::debug!("Transaction beginning");

        let conn = self.db();
        let _guard = conn.start_transaction()?;

        match fun(self) {
            Ok(value) => {
                conn.raw_query_write(|conn| {
                    <C::Connection as Connection>::TransactionManager::commit_transaction(
                        &mut *conn,
                    )
                })?;
                tracing::debug!("Transaction being committed");
                Ok(value)
            }
            Err(err) => {
                tracing::debug!("Transaction being rolled back");
                let result = conn.raw_query_write(|conn| {
                    <C::Connection as Connection>::TransactionManager::rollback_transaction(
                        &mut *conn,
                    )
                });
                match result {
                    Ok(()) => Err(err),
                    Err(crate::ConnectionError::Database(
                        diesel::result::Error::BrokenTransactionManager,
                    )) => Err(err),
                    Err(rollback) => Err(rollback.into()),
                }
            }
        }
    }
}

impl XmtpOpenMlsProvider {
    pub fn new_crypto() -> RustCrypto {
        RustCrypto::default()
    }
}

impl<C> MlsProviderExt for XmtpOpenMlsProvider<C>
where
    C: ConnectionExt,
{
    type Connection = C;

    #[tracing::instrument(level = "debug", skip_all)]
    fn transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProvider<C>) -> Result<T, E>,
        E: From<crate::ConnectionError> + std::error::Error,
    {
        XmtpOpenMlsProvider::<C>::inner_transaction(self, fun)
    }

    fn db(&self) -> &DbConnection<C> {
        self.key_store.db()
    }

    fn key_store(&self) -> &SqlKeyStore<C> {
        &self.key_store
    }
}

impl<C> MlsProviderExt for &XmtpOpenMlsProvider<C>
where
    C: ConnectionExt,
{
    type Connection = C;

    #[tracing::instrument(level = "debug", skip_all)]
    fn transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProvider<C>) -> Result<T, E>,
        E: std::error::Error + From<crate::ConnectionError>,
    {
        XmtpOpenMlsProvider::<C>::inner_transaction(self, fun)
    }

    fn db(&self) -> &DbConnection<C> {
        self.key_store.db()
    }

    fn key_store(&self) -> &SqlKeyStore<C> {
        &self.key_store
    }
}

impl<C> OpenMlsProvider for XmtpOpenMlsProvider<C>
where
    C: ConnectionExt,
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

impl<C> OpenMlsProvider for &XmtpOpenMlsProvider<C>
where
    C: ConnectionExt,
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
