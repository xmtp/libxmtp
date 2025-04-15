use crate::StorageError;
use crate::database::DefaultDatabase;
use crate::{
    ProviderTransactions, XmtpDb, db_connection::DbConnectionPrivate, sql_key_store::SqlKeyStore,
};
use diesel::connection::TransactionManager;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;
use std::marker::PhantomData;

pub type XmtpOpenMlsProvider =
    XmtpOpenMlsProviderPrivate<DefaultDatabase, crate::database::RawDbConnection>;

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

impl<Db> ProviderTransactions<Db> for XmtpOpenMlsProviderPrivate<Db, <Db as XmtpDb>::Connection>
where
    Db: XmtpDb,
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
        F: FnOnce(&XmtpOpenMlsProviderPrivate<Db, <Db as XmtpDb>::Connection>) -> Result<T, E>,
        E: From<StorageError> + std::error::Error,
    {
        tracing::debug!("Transaction beginning");

        let conn = self.conn_ref();
        let _guard = conn.start_transaction::<Db>()?;

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
                    Err(diesel::result::Error::BrokenTransactionManager) => Err(err),
                    Err(rollback) => Err(StorageError::from(rollback).into()),
                }
            }
        }
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
