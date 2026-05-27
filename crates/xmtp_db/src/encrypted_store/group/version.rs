use crate::ConnectionExt;

use super::*;

use xmtp_proto::types::GroupId;
pub trait QueryGroupVersion {
    fn set_group_paused(&self, group_id: &GroupId, min_version: &str) -> Result<(), StorageError>;

    fn unpause_group(&self, group_id: &GroupId) -> Result<(), StorageError>;

    fn get_group_paused_version(&self, group_id: &GroupId) -> Result<Option<String>, StorageError>;

    /// Return every group currently flagged as paused, with the
    /// `paused_for_version` floor it's pinned to. Used by the
    /// startup/sweep recovery path to re-evaluate paused groups
    /// against the now-current `pkg_version` without having to sync
    /// each group individually.
    fn get_paused_groups_with_versions(&self) -> Result<Vec<(GroupId, String)>, StorageError>;
}

impl<T> QueryGroupVersion for &T
where
    T: QueryGroupVersion,
{
    fn set_group_paused(&self, group_id: &GroupId, min_version: &str) -> Result<(), StorageError> {
        (**self).set_group_paused(group_id, min_version)
    }

    fn unpause_group(&self, group_id: &GroupId) -> Result<(), StorageError> {
        (**self).unpause_group(group_id)
    }

    fn get_group_paused_version(&self, group_id: &GroupId) -> Result<Option<String>, StorageError> {
        (**self).get_group_paused_version(group_id)
    }

    fn get_paused_groups_with_versions(&self) -> Result<Vec<(GroupId, String)>, StorageError> {
        (**self).get_paused_groups_with_versions()
    }
}

impl<C: ConnectionExt> QueryGroupVersion for DbConnection<C> {
    fn set_group_paused(&self, group_id: &GroupId, min_version: &str) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;

        self.raw_query(|conn| {
            diesel::update(dsl::groups.filter(dsl::id.eq(group_id)))
                .set(dsl::paused_for_version.eq(Some(min_version.to_string())))
                .execute(conn)
        })?;

        Ok(())
    }

    fn unpause_group(&self, group_id: &GroupId) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;

        self.raw_query(|conn| {
            diesel::update(dsl::groups.filter(dsl::id.eq(group_id)))
                .set(dsl::paused_for_version.eq::<Option<String>>(None))
                .execute(conn)
        })?;

        Ok(())
    }

    fn get_group_paused_version(&self, group_id: &GroupId) -> Result<Option<String>, StorageError> {
        use crate::schema::groups::dsl;

        let paused_version = self.raw_query(|conn| {
            dsl::groups
                .select(dsl::paused_for_version)
                .filter(dsl::id.eq(group_id))
                .first::<Option<String>>(conn)
        })?;

        Ok(paused_version)
    }

    fn get_paused_groups_with_versions(&self) -> Result<Vec<(GroupId, String)>, StorageError> {
        use crate::schema::groups::dsl;

        let rows: Vec<(Vec<u8>, Option<String>)> = self.raw_query(|conn| {
            dsl::groups
                .select((dsl::id, dsl::paused_for_version))
                .filter(dsl::paused_for_version.is_not_null())
                .load::<(Vec<u8>, Option<String>)>(conn)
        })?;

        Ok(rows
            .into_iter()
            .filter_map(|(id, version)| {
                let v = version?;
                match GroupId::try_from(id.as_slice()) {
                    Ok(group_id) => Some((group_id, v)),
                    Err(err) => {
                        tracing::warn!(
                            error = %err,
                            id_hex = %hex::encode(&id),
                            id_len = id.len(),
                            "get_paused_groups_with_versions: skipping row with \
                             unparseable group id (not 16 bytes)"
                        );
                        None
                    }
                }
            })
            .collect())
    }
}
