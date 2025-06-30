use crate::MlsProviderExt;
use crate::sql_key_store::SqlKeyStoreError;
use crate::{ConnectionExt, DbQuery};
use diesel::Connection;
use diesel::connection::TransactionManager;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;
use openmls_traits::storage::CURRENT_VERSION;
use openmls_traits::storage::StorageProvider;

/// Convenience super trait to constrain the storage provider to a
/// specific error type and version
/// This storage provider is likewise implemented on both &T and T references,
/// to allow creating a referenced or owned provider.
// constraining the error type here will avoid leaking
// the associated type parameter, so we don't need to define it on every function.
pub trait XmtpMlsStorageProvider:
    StorageProvider<CURRENT_VERSION, Error = SqlKeyStoreError>
{
}

impl<'a, T> XmtpMlsStorageProvider for T where
    T: ?Sized + StorageProvider<CURRENT_VERSION, Error = SqlKeyStoreError>
{
}

pub struct XmtpOpenMlsProvider<S> {
    crypto: RustCrypto,
    mls_storage: S,
}

impl<S> XmtpOpenMlsProvider<S> {
    pub fn new(mls_storage: S) -> Self {
        Self {
            crypto: RustCrypto::default(),
            mls_storage,
        }
    }
}

impl<S> XmtpOpenMlsProvider<S>
where
    S: XmtpMlsStorageProvider,
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
    S: XmtpMlsStorageProvider,
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
        &self.mls_storage
    }
}

impl<S> MlsProviderExt for &XmtpOpenMlsProvider<S>
where
    S: XmtpMlsStorageProvider,
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
        &self.mls_storage
    }
}

impl<S> OpenMlsProvider for XmtpOpenMlsProvider<S>
where
    S: XmtpMlsStorageProvider,
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
        &self.mls_storage
    }
}

impl<S> OpenMlsProvider for &XmtpOpenMlsProvider<S>
where
    S: XmtpMlsStorageProvider,
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
        &self.mls_storage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openmls_memory_storage::MemoryStorage;
    /*
        fn create_provider_with_reference() {
            let storage = MemoryStorage::default();
            XmtpOpenMlsProvider::new(storage)
        }
    */
}
