use crate::MlsProviderExt;
use crate::{ConnectionExt, DbQuery};
use diesel::Connection;
use diesel::connection::TransactionManager;
use openmls::storage::StorageProvider;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

pub struct XmtpOpenMlsProvider<S> {
    crypto: RustCrypto,
    key_store: S,
}

impl<S> XmtpOpenMlsProvider<S> {
    pub fn new(key_store: S) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store,
        }
    }
}

impl<S> XmtpOpenMlsProvider<S>
where
    S: StorageProvider,
{
    #[tracing::instrument(level = "debug", skip_all)]
    fn inner_transaction<T, F, E, C, D>(&self, conn: &D, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProvider<S>) -> Result<T, E>,
        E: From<crate::ConnectionError> + std::error::Error,
        C: ConnectionExt,
        D: DbQuery<C>,
    {
        tracing::debug!("Transaction beginning");

        let _guard = conn.start_transaction()?;

        match fun(self) {
            Ok(value) => {
                conn.raw_query_write(|conn| {
                    <<D as ConnectionExt>::Connection as Connection>::TransactionManager::commit_transaction(
                        &mut *conn,
                    )
                })?;
                tracing::debug!("Transaction being committed");
                Ok(value)
            }
            Err(err) => {
                tracing::debug!("Transaction being rolled back");
                let result = conn.raw_query_write(|conn| {
                    <<D as ConnectionExt>::Connection as Connection>::TransactionManager::rollback_transaction(
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

impl<S> XmtpOpenMlsProvider<S> {
    pub fn new_crypto() -> RustCrypto {
        RustCrypto::default()
    }
}

impl<S> MlsProviderExt for XmtpOpenMlsProvider<S>
where
    S: StorageProvider,
{
    #[tracing::instrument(level = "debug", skip_all)]
    fn transaction<T, F, E, C, D>(&self, conn: &D, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProvider<S>) -> Result<T, E>,
        E: From<crate::ConnectionError> + std::error::Error,
        C: ConnectionExt,
        D: DbQuery<C>,
    {
        XmtpOpenMlsProvider::inner_transaction(self, conn, fun)
    }

    fn key_store(&self) -> &<Self as OpenMlsProvider>::StorageProvider {
        &self.key_store
    }
}

impl<S> MlsProviderExt for &XmtpOpenMlsProvider<S>
where
    S: StorageProvider,
{
    #[tracing::instrument(level = "debug", skip_all)]
    fn transaction<T, F, E, C, D>(&self, conn: &D, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProvider<S>) -> Result<T, E>,
        E: std::error::Error + From<crate::ConnectionError>,
        C: ConnectionExt,
        D: DbQuery<C>,
    {
        XmtpOpenMlsProvider::inner_transaction(self, conn, fun)
    }

    fn key_store(&self) -> &<Self as OpenMlsProvider>::StorageProvider {
        &self.key_store
    }
}

impl<S> OpenMlsProvider for XmtpOpenMlsProvider<S>
where
    S: StorageProvider,
{
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = S;
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

impl<S> OpenMlsProvider for &XmtpOpenMlsProvider<S>
where
    S: StorageProvider,
{
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = S;

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
