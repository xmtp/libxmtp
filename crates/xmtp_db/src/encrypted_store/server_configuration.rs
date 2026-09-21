//! The one row that binds this database to one backend deployment.
//!
//! Spec 006 §6.2. The row holds the deployment identifier, the URL the copy
//! came from, the serialized response, and when it was fetched. A conflicting
//! identifier is recorded once, by the conflict path only, and is never
//! cleared.

use crate::encrypted_store::schema::server_configuration;
use crate::schema::server_configuration::dsl;
use crate::{ConnectionExt, DbConnection, StorageError};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

/// The single stored row. `id` is always zero; the table's check constraint
/// enforces it.
#[derive(Insertable, Queryable, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[diesel(table_name = server_configuration)]
pub struct StoredServerConfiguration {
    pub id: i32,
    pub identifier: String,
    pub backend_url: String,
    /// The serialized `GetConfigurationResponse`, stored whole.
    pub response: Vec<u8>,
    pub fetched_at_ns: i64,
    /// An identifier that did not match `identifier`. Set once, never cleared.
    pub conflicting_identifier: Option<String>,
}

pub trait QueryServerConfiguration {
    /// The stored copy, or `None` when this database has never held one.
    fn server_configuration(&self) -> Result<Option<StoredServerConfiguration>, StorageError>;

    /// Write the copy whole: identifier, URL, response, and fetch time. Never
    /// touches `conflicting_identifier`, so a matching refresh cannot erase a
    /// recorded conflict.
    fn store_server_configuration(
        &self,
        identifier: &str,
        backend_url: &str,
        response: &[u8],
        fetched_at_ns: i64,
    ) -> Result<(), StorageError>;

    /// Record that the deployment answered with a different identifier.
    /// Does nothing when no row exists yet.
    fn record_server_configuration_conflict(
        &self,
        conflicting_identifier: &str,
    ) -> Result<(), StorageError>;
}

impl<T> QueryServerConfiguration for &T
where
    T: QueryServerConfiguration,
{
    fn server_configuration(&self) -> Result<Option<StoredServerConfiguration>, StorageError> {
        (**self).server_configuration()
    }

    fn store_server_configuration(
        &self,
        identifier: &str,
        backend_url: &str,
        response: &[u8],
        fetched_at_ns: i64,
    ) -> Result<(), StorageError> {
        (**self).store_server_configuration(identifier, backend_url, response, fetched_at_ns)
    }

    fn record_server_configuration_conflict(
        &self,
        conflicting_identifier: &str,
    ) -> Result<(), StorageError> {
        (**self).record_server_configuration_conflict(conflicting_identifier)
    }
}

impl<C: ConnectionExt> QueryServerConfiguration for DbConnection<C> {
    fn server_configuration(&self) -> Result<Option<StoredServerConfiguration>, StorageError> {
        Ok(self.raw_query(|conn| {
            dsl::server_configuration
                .first::<StoredServerConfiguration>(conn)
                .optional()
        })?)
    }

    // implements: CONF-031
    fn store_server_configuration(
        &self,
        identifier: &str,
        backend_url: &str,
        response: &[u8],
        fetched_at_ns: i64,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::insert_into(dsl::server_configuration)
                .values((
                    dsl::id.eq(0),
                    dsl::identifier.eq(identifier),
                    dsl::backend_url.eq(backend_url),
                    dsl::response.eq(response),
                    dsl::fetched_at_ns.eq(fetched_at_ns),
                ))
                .on_conflict(dsl::id)
                .do_update()
                .set((
                    dsl::identifier.eq(identifier),
                    dsl::backend_url.eq(backend_url),
                    dsl::response.eq(response),
                    dsl::fetched_at_ns.eq(fetched_at_ns),
                ))
                .execute(conn)
        })?;
        Ok(())
    }

    fn record_server_configuration_conflict(
        &self,
        conflicting_identifier: &str,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::server_configuration)
                .set(dsl::conflicting_identifier.eq(conflicting_identifier))
                .execute(conn)
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
