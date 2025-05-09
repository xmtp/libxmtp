use crate::ConnectionExt;

use super::*;

impl<C: ConnectionExt> DbConnection<C> {
    pub fn set_group_paused(&self, group_id: &[u8], min_version: &str) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;

        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.filter(dsl::id.eq(group_id)))
                .set(dsl::paused_for_version.eq(Some(min_version.to_string())))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn unpause_group(&self, group_id: &[u8]) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;

        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.filter(dsl::id.eq(group_id)))
                .set(dsl::paused_for_version.eq::<Option<String>>(None))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn get_group_paused_version(
        &self,
        group_id: &[u8],
    ) -> Result<Option<String>, StorageError> {
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
