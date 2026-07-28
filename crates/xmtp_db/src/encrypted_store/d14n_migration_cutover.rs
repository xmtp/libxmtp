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

#[maybe_async::maybe_async(AFIT)]
pub trait QueryMigrationCutover {
    async fn get_migration_cutover(&self) -> Result<StoredMigrationCutover, StorageError>;

    async fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), StorageError>;

    async fn get_last_checked_ns(&self) -> Result<i64, StorageError>;

    async fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), StorageError>;

    async fn set_has_migrated(&self, has_migrated: bool) -> Result<(), StorageError>;
}

#[maybe_async::maybe_async(AFIT)]
impl<T: QueryMigrationCutover> QueryMigrationCutover for &T {
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
    fn get_migration_cutover(&self) -> Result<StoredMigrationCutover, StorageError> {
        let result = self.raw_query(|conn| dsl::d14n_migration_cutover.first(conn).optional())?;
        Ok(result.unwrap_or_default())
    }

    fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::d14n_migration_cutover.find(1))
                .set(d14n_migration_cutover::cutover_ns.eq(cutover_ns))
                .execute(conn)
        })?;
        Ok(())
    }

    fn get_last_checked_ns(&self) -> Result<i64, StorageError> {
        let cutover = self.get_migration_cutover()?;
        Ok(cutover.last_checked_ns)
    }

    fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::d14n_migration_cutover.find(1))
                .set(d14n_migration_cutover::last_checked_ns.eq(last_checked_ns))
                .execute(conn)
        })?;
        Ok(())
    }

    fn set_has_migrated(&self, has_migrated: bool) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::d14n_migration_cutover.find(1))
                .set(d14n_migration_cutover::has_migrated.eq(has_migrated))
                .execute(conn)
        })?;
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
