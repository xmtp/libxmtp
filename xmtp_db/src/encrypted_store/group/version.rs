use crate::ConnectionExt;

use super::*;

pub trait QueryGroupVersion {
    fn set_group_paused(&self, group_id: &[u8], min_version: &str) -> Result<(), StorageError>;

    fn unpause_group(&self, group_id: &[u8]) -> Result<(), StorageError>;

    fn get_group_paused_version(&self, group_id: &[u8]) -> Result<Option<String>, StorageError>;
}

impl<T> QueryGroupVersion for &T
where
    T: QueryGroupVersion,
{
    fn set_group_paused(&self, group_id: &[u8], min_version: &str) -> Result<(), StorageError> {
        (**self).set_group_paused(group_id, min_version)
    }

    fn unpause_group(&self, group_id: &[u8]) -> Result<(), StorageError> {
        (**self).unpause_group(group_id)
    }

    fn get_group_paused_version(&self, group_id: &[u8]) -> Result<Option<String>, StorageError> {
        (**self).get_group_paused_version(group_id)
    }
}

impl<C: ConnectionExt> QueryGroupVersion for DbConnection<C> {
    fn set_group_paused(&self, group_id: &[u8], min_version: &str) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;

        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.filter(dsl::id.eq(group_id)))
                .set(dsl::paused_for_version.eq(Some(min_version.to_string())))
                .execute(conn)
        })?;

        Ok(())
    }

    fn unpause_group(&self, group_id: &[u8]) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;

        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.filter(dsl::id.eq(group_id)))
                .set(dsl::paused_for_version.eq::<Option<String>>(None))
                .execute(conn)
        })?;

        Ok(())
    }

    fn get_group_paused_version(&self, group_id: &[u8]) -> Result<Option<String>, StorageError> {
        use crate::schema::groups::dsl;

        let paused_version = self.raw_query_read(|conn| {
            dsl::groups
                .select(dsl::paused_for_version)
                .filter(dsl::id.eq(group_id))
                .first::<Option<String>>(conn)
        })?;

        Ok(paused_version)
    }
}
