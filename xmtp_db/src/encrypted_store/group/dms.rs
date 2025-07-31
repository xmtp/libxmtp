use crate::ConnectionExt;

use super::*;
use crate::ConnectionError;

pub trait QueryDms {
    /// Same behavior as fetched, but will stitch DM groups
    fn fetch_stitched(&self, key: &[u8]) -> Result<Option<StoredGroup>, ConnectionError>;

    fn find_dm_group<M>(&self, members: M) -> Result<Option<StoredGroup>, ConnectionError>
    where
        M: std::fmt::Display;

    /// Load the other DMs that are stitched into this group
    fn other_dms(&self, group_id: &[u8]) -> Result<Vec<StoredGroup>, ConnectionError>;
}

impl<T> QueryDms for &T where T: QueryDms {
    fn fetch_stitched(&self, key: &[u8]) -> Result<Option<StoredGroup>, ConnectionError> {
        (**self).fetch_stitched(key)
    }

    fn find_dm_group<M>(&self, members: M) -> Result<Option<StoredGroup>, ConnectionError>
    where
        M: std::fmt::Display {
        (**self).find_dm_group(members)
    }

    fn other_dms(&self, group_id: &[u8]) -> Result<Vec<StoredGroup>, ConnectionError> {
        (**self).other_dms(group_id)
    }
}

impl<C: ConnectionExt> QueryDms for DbConnection<C> {
    /// Same behavior as fetched, but will stitch DM groups
    fn fetch_stitched(&self, key: &[u8]) -> Result<Option<StoredGroup>, ConnectionError> {
        let group = self.raw_query_read(|conn| {
            groups::table
                .filter(groups::id.eq(key))
                .first::<StoredGroup>(conn)
                .optional()
        })?;

        // Is this group a DM?
        let Some(StoredGroup {
            dm_id: Some(dm_id), ..
        }) = group
        else {
            // If not, return the group
            return Ok(group);
        };

        // Otherwise, return the stitched DM
        self.raw_query_read(|conn| {
            groups::table
                .filter(groups::dm_id.eq(dm_id))
                .order_by(groups::last_message_ns.desc())
                .first::<StoredGroup>(conn)
                .optional()
        })
    }

    fn find_dm_group<M>(&self, members: M) -> Result<Option<StoredGroup>, ConnectionError>
    where
        M: std::fmt::Display,
    {
        let query = dsl::groups
            .filter(dsl::dm_id.eq(Some(members.to_string())))
            .order_by(dsl::last_message_ns.desc());

        self.raw_query_read(|conn| query.first(conn).optional())
    }

    /// Load the other DMs that are stitched into this group
    fn other_dms(&self, group_id: &[u8]) -> Result<Vec<StoredGroup>, ConnectionError> {
        let query = dsl::groups.filter(dsl::id.eq(group_id));
        let groups: Vec<StoredGroup> = self.raw_query_read(|conn| query.load(conn))?;

        // Grab the dm_id of the group
        let Some(StoredGroup {
            id,
            dm_id: Some(dm_id),
            ..
        }) = groups.into_iter().next()
        else {
            return Ok(vec![]);
        };

        let query = dsl::groups
            .filter(dsl::dm_id.eq(dm_id))
            .filter(dsl::id.ne(id));

        let other_dms: Vec<StoredGroup> = self.raw_query_read(|conn| query.load(conn))?;
        Ok(other_dms)
    }
}

#[cfg(test)]
pub(super) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use super::*;
    use crate::{Store, test_utils::with_connection};
    use std::sync::atomic::{AtomicU16, Ordering};
    use xmtp_common::{rand_vec, time::now_ns};

    static TARGET_INBOX_ID: AtomicU16 = AtomicU16::new(2);

    /// Generate a test dm group
    pub fn generate_dm(state: Option<GroupMembershipState>) -> StoredGroup {
        StoredGroup::builder()
            .id(rand_vec::<24>())
            .created_at_ns(now_ns())
            .membership_state(state.unwrap_or(GroupMembershipState::Allowed))
            .added_by_inbox_id("placeholder_address")
            .dm_id(format!(
                "dm:placeholder_inbox_id_1:placeholder_inbox_id_{}",
                TARGET_INBOX_ID.fetch_add(1, Ordering::SeqCst)
            ))
            .build()
            .unwrap()
    }

    #[xmtp_common::test]
    async fn test_dm_stitching() {
        with_connection(|conn| {
            StoredGroup::builder()
                .id(rand_vec::<24>())
                .created_at_ns(now_ns())
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("placeholder_address")
                .dm_id(Some("dm:some_wise_guy:thats_me".to_string()))
                .build()
                .unwrap()
                .store(conn)
                .unwrap();

            StoredGroup::builder()
                .id(rand_vec::<24>())
                .created_at_ns(now_ns())
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("placeholder_address")
                .dm_id(Some("dm:some_wise_guy:thats_me".to_string()))
                .build()
                .unwrap()
                .store(conn)
                .unwrap();
            let all_groups = conn.find_groups(GroupQueryArgs::default()).unwrap();

            assert_eq!(all_groups.len(), 1);
        })
        .await
    }
}
