#[cfg(feature = "sync")]
use super::{
    ConnectionExt,
    db_connection::DbConnection,
    schema::d14n_migration_cutover::{self, dsl},
};
use crate::StorageError;
#[cfg(feature = "sync")]
use diesel::prelude::*;

#[derive(Debug, Clone)]
#[cfg_attr(
    feature = "sync",
    derive(Identifiable, Insertable, Queryable, AsChangeset)
)]
#[cfg_attr(feature = "sync", diesel(table_name = d14n_migration_cutover))]
#[cfg_attr(feature = "sync", diesel(primary_key(id)))]
pub struct StoredMigrationCutover {
    pub id: i32,
    pub cutover_ns: i64,
    pub last_checked_ns: i64,
    pub has_migrated: bool,
}

impl Default for StoredMigrationCutover {
    fn default() -> Self {
        Self {
            id: 1,
            cutover_ns: i64::MAX,
            last_checked_ns: 0,
            has_migrated: false,
        }
    }
}

pub trait QueryMigrationCutover {
    fn get_migration_cutover(
        &self,
    ) -> impl std::future::Future<Output = Result<StoredMigrationCutover, StorageError>>
    + xmtp_common::MaybeSend;

    fn set_cutover_ns(
        &self,
        cutover_ns: i64,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn get_last_checked_ns(
        &self,
    ) -> impl std::future::Future<Output = Result<i64, StorageError>> + xmtp_common::MaybeSend;

    fn set_last_checked_ns(
        &self,
        last_checked_ns: i64,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn set_has_migrated(
        &self,
        has_migrated: bool,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;
}

impl<T: QueryMigrationCutover + xmtp_common::MaybeSync> QueryMigrationCutover for &T {
    async fn get_migration_cutover(&self) -> Result<StoredMigrationCutover, StorageError> {
        (**self).get_migration_cutover().await
    }

    async fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), StorageError> {
        (**self).set_cutover_ns(cutover_ns).await
    }

    async fn get_last_checked_ns(&self) -> Result<i64, StorageError> {
        (**self).get_last_checked_ns().await
    }

    async fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), StorageError> {
        (**self).set_last_checked_ns(last_checked_ns).await
    }

    async fn set_has_migrated(&self, has_migrated: bool) -> Result<(), StorageError> {
        (**self).set_has_migrated(has_migrated).await
    }
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryMigrationCutover for DbConnection<C> {
    async fn get_migration_cutover(&self) -> Result<StoredMigrationCutover, StorageError> {
        let result = self.raw_query(|conn| dsl::d14n_migration_cutover.first(conn).optional())?;
        Ok(result.unwrap_or_default())
    }

    async fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::d14n_migration_cutover.find(1))
                .set(d14n_migration_cutover::cutover_ns.eq(cutover_ns))
                .execute(conn)
        })?;
        Ok(())
    }

    async fn get_last_checked_ns(&self) -> Result<i64, StorageError> {
        let cutover = self.get_migration_cutover().await?;
        Ok(cutover.last_checked_ns)
    }

    async fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::d14n_migration_cutover.find(1))
                .set(d14n_migration_cutover::last_checked_ns.eq(last_checked_ns))
                .execute(conn)
        })?;
        Ok(())
    }

    async fn set_has_migrated(&self, has_migrated: bool) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::d14n_migration_cutover.find(1))
                .set(d14n_migration_cutover::has_migrated.eq(has_migrated))
                .execute(conn)
        })?;
        Ok(())
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
impl QueryMigrationCutover for crate::pg::PgDb {
    /// The migration seeds row 1, so the `unwrap_or_default` here is a fallback
    /// for a database that predates it rather than the normal path.
    async fn get_migration_cutover(&self) -> Result<StoredMigrationCutover, StorageError> {
        use sqlx::Row;
        let mut c = self.conn().await?;
        let row = sqlx::query(
            "SELECT id, cutover_ns, last_checked_ns, has_migrated \
             FROM d14n_migration_cutover LIMIT 1",
        )
        .fetch_optional(&mut *c)
        .await
        .map_err(crate::ConnectionError::from)?;

        let Some(row) = row else {
            return Ok(StoredMigrationCutover::default());
        };
        Ok(StoredMigrationCutover {
            id: row.try_get(0).map_err(crate::ConnectionError::from)?,
            cutover_ns: row.try_get(1).map_err(crate::ConnectionError::from)?,
            last_checked_ns: row.try_get(2).map_err(crate::ConnectionError::from)?,
            has_migrated: row.try_get(3).map_err(crate::ConnectionError::from)?,
        })
    }

    async fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), StorageError> {
        let mut c = self.conn().await?;
        sqlx::query("UPDATE d14n_migration_cutover SET cutover_ns = $1 WHERE id = 1")
            .bind(cutover_ns)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
        Ok(())
    }

    async fn get_last_checked_ns(&self) -> Result<i64, StorageError> {
        Ok(self.get_migration_cutover().await?.last_checked_ns)
    }

    async fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), StorageError> {
        let mut c = self.conn().await?;
        sqlx::query("UPDATE d14n_migration_cutover SET last_checked_ns = $1 WHERE id = 1")
            .bind(last_checked_ns)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
        Ok(())
    }

    async fn set_has_migrated(&self, has_migrated: bool) -> Result<(), StorageError> {
        let mut c = self.conn().await?;
        sqlx::query("UPDATE d14n_migration_cutover SET has_migrated = $1 WHERE id = 1")
            .bind(has_migrated)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::with_connection;

    #[xmtp_common::test]
    fn test_default_migration_cutover() {
        with_connection(|conn| {
            let cutover = conn.get_migration_cutover().unwrap();
            assert_eq!(cutover.cutover_ns, i64::MAX);
            assert_eq!(cutover.last_checked_ns, 0);
            assert!(!cutover.has_migrated);
        })
    }

    #[xmtp_common::test]
    fn test_set_cutover_ns() {
        with_connection(|conn| {
            let timestamp = 1_700_000_000_000_000_000i64;
            conn.set_cutover_ns(timestamp).unwrap();

            let cutover = conn.get_migration_cutover().unwrap();
            assert_eq!(cutover.cutover_ns, timestamp);
            assert_eq!(cutover.last_checked_ns, 0);
            assert!(!cutover.has_migrated);
        })
    }

    #[xmtp_common::test]
    fn test_set_last_checked_ns() {
        with_connection(|conn| {
            let timestamp = 1_700_000_000_000_000_000i64;
            conn.set_last_checked_ns(timestamp).unwrap();

            let cutover = conn.get_migration_cutover().unwrap();
            assert_eq!(cutover.cutover_ns, i64::MAX);
            assert_eq!(cutover.last_checked_ns, timestamp);
            assert!(!cutover.has_migrated);
        })
    }

    #[xmtp_common::test]
    fn test_get_last_checked_ns() {
        with_connection(|conn| {
            let timestamp = 1_700_000_000_000_000_000i64;
            conn.set_last_checked_ns(timestamp).unwrap();

            let last_checked = conn.get_last_checked_ns().unwrap();
            assert_eq!(last_checked, timestamp);
        })
    }

    #[xmtp_common::test]
    fn test_set_has_migrated() {
        with_connection(|conn| {
            conn.set_has_migrated(true).unwrap();

            let cutover = conn.get_migration_cutover().unwrap();
            assert!(cutover.has_migrated);
        })
    }
}
