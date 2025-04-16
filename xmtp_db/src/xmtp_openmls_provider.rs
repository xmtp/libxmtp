use crate::database::DefaultDatabase;
use crate::{ConnectionExt, StorageError};
use crate::{
    ProviderTransactions, XmtpDb, db_connection::DbConnectionPrivate, sql_key_store::SqlKeyStore,
};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

pub type XmtpOpenMlsProvider = XmtpOpenMlsProviderPrivate<crate::database::DefaultConnection>;

#[derive(Debug)]
pub struct XmtpOpenMlsProviderPrivate<C> {
    crypto: RustCrypto,
    key_store: SqlKeyStore<C>,
}

impl<C> XmtpOpenMlsProviderPrivate<C> {
    pub fn new(conn: C) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: SqlKeyStore::new(conn),
        }
    }

    pub fn new_crypto() -> RustCrypto {
        RustCrypto::default()
    }

    pub fn conn_ref(&self) -> &C {
        self.key_store.conn_ref()
    }
}

impl<C> ProviderTransactions<C> for XmtpOpenMlsProviderPrivate<C>
where
    C: ConnectionExt,
{
    /// Start a new database transaction with the OpenMLS Provider from XMTP
    /// with the provided connection
    /// # Arguments
    /// `fun`: Scoped closure providing a MLSProvider to carry out the transaction
    ///
    /// # Examples
    ///
    /// ```ignore
    /// provider.transaction(|provider| {
    ///     // do some operations requiring provider
    ///     // access the connection with .conn()
    ///     provider.conn().db_operation()?;
    /// })
    /// ```
    fn transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProviderPrivate<C>) -> Result<T, E>,
        E: From<StorageError> + std::error::Error,
    {
        tracing::debug!("Transaction beginning");

        let conn = self.conn_ref();
        let _guard = conn.start_transaction()?;

        match fun(self) {
            Ok(value) => {
                conn.raw_query_write(|conn| {
                    <Db as XmtpDb>::TransactionManager::commit_transaction(&mut *conn)
                })
                .map_err(StorageError::from)?;
                tracing::debug!("Transaction being committed");
                Ok(value)
            }
            Err(err) => {
                tracing::debug!("Transaction being rolled back");
                match conn.raw_query_write(|conn| {
                    <Db as XmtpDb>::TransactionManager::rollback_transaction(&mut *conn)
                }) {
                    Ok(()) => Err(err),
                    Err(StorageError::DieselResult(
                        diesel::result::Error::BrokenTransactionManager,
                    )) => Err(err),
                    Err(rollback) => Err(StorageError::from(rollback).into()),
                }
            }
        }
    }
}

impl<C> OpenMlsProvider for XmtpOpenMlsProviderPrivate<C>
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

impl<C> OpenMlsProvider for &XmtpOpenMlsProviderPrivate<C>
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
