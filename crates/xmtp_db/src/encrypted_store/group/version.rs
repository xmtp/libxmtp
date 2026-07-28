use crate::ConnectionExt;

use super::*;

use xmtp_proto::types::GroupId;
#[maybe_async::maybe_async(AFIT)]
pub trait QueryGroupVersion {
    async fn set_group_paused(
        &self,
        group_id: &GroupId,
        min_version: &str,
    ) -> Result<(), StorageError>;

    async fn unpause_group(&self, group_id: &GroupId) -> Result<(), StorageError>;

    async fn get_group_paused_version(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<String>, StorageError>;

    /// Return every group currently flagged as paused, with the
    /// `paused_for_version` floor it's pinned to. Used by the
    /// startup/sweep recovery path to re-evaluate paused groups
    /// against the now-current `pkg_version` without having to sync
    /// each group individually.
    async fn get_paused_groups_with_versions(&self)
    -> Result<Vec<(GroupId, String)>, StorageError>;
}

#[maybe_async::maybe_async(AFIT)]
impl<T> QueryGroupVersion for &T
where
    T: QueryGroupVersion,
{
    async fn set_group_paused(
        &self,
        group_id: &GroupId,
        min_version: &str,
    ) -> Result<(), StorageError> {
        (**self).set_group_paused(group_id, min_version).await
    }

    async fn unpause_group(&self, group_id: &GroupId) -> Result<(), StorageError> {
        (**self).unpause_group(group_id).await
    }

    async fn get_group_paused_version(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<String>, StorageError> {
        (**self).get_group_paused_version(group_id).await
    }

    async fn get_paused_groups_with_versions(
        &self,
    ) -> Result<Vec<(GroupId, String)>, StorageError> {
        (**self).get_paused_groups_with_versions().await
    }
}

#[cfg(feature = "sync")]
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

/// sqlx backend -- Postgres only. Gated `not(feature = "sync")` because the two
/// tracks are single-choice but not hard-exclusive: cargo feature unification can
/// hand a graph both, and `maybe-async/is_sync` is global, so when both are on
/// the trait has already collapsed to the blocking shape.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
impl QueryGroupVersion for crate::pg::PgDb {
    async fn set_group_paused(
        &self,
        group_id: &GroupId,
        min_version: &str,
    ) -> Result<(), StorageError> {
        let mut c = self.conn().await?;
        sqlx::query("UPDATE groups SET paused_for_version = $1 WHERE id = $2")
            .bind(min_version)
            .bind(group_id)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
        Ok(())
    }

    async fn unpause_group(&self, group_id: &GroupId) -> Result<(), StorageError> {
        let mut c = self.conn().await?;
        sqlx::query("UPDATE groups SET paused_for_version = NULL WHERE id = $1")
            .bind(group_id)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
        Ok(())
    }

    /// Matches the diesel impl's `first()`: a missing group row is an error, not
    /// `Ok(None)`. `None` means the row exists with a null `paused_for_version`.
    async fn get_group_paused_version(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<String>, StorageError> {
        use sqlx::Row;
        let mut c = self.conn().await?;
        let row = sqlx::query("SELECT paused_for_version FROM groups WHERE id = $1")
            .bind(group_id)
            .fetch_one(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
        Ok(row
            .try_get::<Option<String>, _>(0)
            .map_err(crate::ConnectionError::from)?)
    }

    async fn get_paused_groups_with_versions(
        &self,
    ) -> Result<Vec<(GroupId, String)>, StorageError> {
        use sqlx::Row;
        let mut c = self.conn().await?;
        let rows = sqlx::query(
            "SELECT id, paused_for_version FROM groups WHERE paused_for_version IS NOT NULL",
        )
        .fetch_all(&mut *c)
        .await
        .map_err(crate::ConnectionError::from)?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let id: Vec<u8> = row.try_get(0).ok()?;
                let version: Option<String> = row.try_get(1).ok()?;
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
