pub mod api_client_wrapper;
pub mod association;
pub mod builder;
pub mod client;
mod configuration;
pub mod identity;
pub mod mock_xmtp_api_client;
pub mod owner;
mod proto_wrapper;
pub mod storage;
pub mod types;
mod xmtp_openmls_provider;

pub use client::{Client, Network};
use storage::StorageError;
use xmtp_cryptography::signature::{RecoverableSignature, SignatureError};

pub trait InboxOwner {
    fn get_address(&self) -> String;
    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError>;
}

// Inserts a model to the underlying data store
pub trait Store<StorageConnection> {
    fn store(&self, into: &mut StorageConnection) -> Result<(), StorageError>;
}

pub trait Fetch<Model> {
    type Key;
    fn fetch(&mut self, key: Self::Key) -> Result<Option<Model>, StorageError>;
}

pub trait Delete<Model> {
    type Key;
    fn delete(&mut self, key: Self::Key) -> Result<usize, StorageError>;
}

#[macro_export]
macro_rules! impl_fetch_and_store {
    ($model:ty, $table:ident) => {
        impl $crate::Store<$crate::storage::encrypted_store::DbConnection> for $model {

            fn store(&self, into: &mut $crate::storage::encrypted_store::DbConnection) -> Result<(), $crate::StorageError> {
                diesel::insert_into($table::table)
                    .values(self)
                    .execute(into)
                    .map_err(|e| $crate::StorageError::from(e))?;
                Ok(())
            }
        }

        impl $crate::Fetch<$model> for $crate::storage::encrypted_store::DbConnection {
            type Key = ();
            fn fetch(&mut self, _key: Self::Key) -> Result<Option<$model>, $crate::StorageError> {
                use $crate::storage::encrypted_store::schema::$table::dsl::*;
                Ok($table.first(self).optional()?)
            }
        }
    };

    ($model:ty, $table:ident, $key:ty) => {
        impl $crate::Store<$crate::storage::encrypted_store::DbConnection> for $model {
            fn store(&self, into: &mut $crate::storage::encrypted_store::DbConnection) -> Result<(), $crate::StorageError> {
                diesel::insert_into($table::table)
                    .values(self)
                    .execute(into)
                    .map_err(|e| $crate::StorageError::from(e))?;
                Ok(())
            }
        }

        impl $crate::Fetch<$model> for $crate::storage::encrypted_store::DbConnection {
            type Key = $key;
            fn fetch(&mut self, key: Self::Key) -> Result<Option<$model>, $crate::StorageError> {
                use $crate::storage::encrypted_store::schema::$table::dsl::*;
                Ok($table.find(key).first(self).optional()?)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use std::sync::Once;
    static INIT: Once = Once::new();
    
    /// Setup for tests
    pub fn setup() {
        INIT.call_once(|| {
            tracing_subscriber::fmt::init();
        })
    }
}

