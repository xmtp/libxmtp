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

impl<T> QueryDms for &T
where
    T: QueryDms,
{
    fn fetch_stitched(&self, key: &[u8]) -> Result<Option<StoredGroup>, ConnectionError> {
        (**self).fetch_stitched(key)
    }

    fn find_dm_group<M>(&self, members: M) -> Result<Option<StoredGroup>, ConnectionError>
    where
        M: std::fmt::Display,
    {
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

    #[xmtp_common::test]
    async fn test_dm_deduplication() {
        with_connection(|conn| {
            let now = now_ns();
            let base_time = now - 1_000_000_000; // 1 second ago

            // Create DM groups with same dm_id but different timestamps
            let dm_id = "dm:alice:bob";

            // Oldest DM (should be filtered out)
            let oldest_dm = StoredGroup::builder()
                .id(rand_vec::<24>())
                .created_at_ns(base_time)
                .last_message_ns(base_time)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("alice")
                .dm_id(Some(dm_id.to_string()))
                .build()
                .unwrap();
            oldest_dm.store(conn).unwrap();

            // Middle DM (should be filtered out)
            let middle_dm = StoredGroup::builder()
                .id(rand_vec::<24>())
                .created_at_ns(base_time + 1_000_000)
                .last_message_ns(base_time + 1_000_000)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("bob")
                .dm_id(Some(dm_id.to_string()))
                .build()
                .unwrap();
            middle_dm.store(conn).unwrap();

            // Latest DM (should be kept)
            let latest_dm = StoredGroup::builder()
                .id(rand_vec::<24>())
                .created_at_ns(base_time + 2_000_000)
                .last_message_ns(base_time + 2_000_000)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("alice")
                .dm_id(Some(dm_id.to_string()))
                .build()
                .unwrap();
            latest_dm.store(conn).unwrap();

            // Create another DM with different dm_id (should always be kept)
            let different_dm = StoredGroup::builder()
                .id(rand_vec::<24>())
                .created_at_ns(base_time + 500_000)
                .last_message_ns(base_time + 500_000)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("charlie")
                .dm_id(Some("dm:charlie:dave".to_string()))
                .build()
                .unwrap();
            different_dm.store(conn).unwrap();

            // Create a regular group (non-DM, should always be kept)
            let regular_group = StoredGroup::builder()
                .id(rand_vec::<24>())
                .created_at_ns(base_time + 1_500_000)
                .last_message_ns(base_time + 1_500_000)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("alice")
                .dm_id(None) // No dm_id = regular group
                .build()
                .unwrap();
            regular_group.store(conn).unwrap();

            // Test with include_duplicate_dms = false (default deduplication)
            let deduplicated_groups = conn
                .find_groups(GroupQueryArgs {
                    include_duplicate_dms: false,
                    ..Default::default()
                })
                .unwrap();

            // Should have 3 groups: latest DM, different DM, and regular group
            assert_eq!(deduplicated_groups.len(), 3);

            // Verify the latest DM is kept (highest last_message_ns for dm_id)
            let kept_dm = deduplicated_groups
                .iter()
                .find(|g| g.dm_id.as_deref() == Some(dm_id))
                .expect("Should find the DM group");
            assert_eq!(kept_dm.id, latest_dm.id);
            assert_eq!(kept_dm.last_message_ns, Some(base_time + 2_000_000));

            // Verify different DM is kept
            let kept_different_dm = deduplicated_groups
                .iter()
                .find(|g| g.dm_id.as_deref() == Some("dm:charlie:dave"))
                .expect("Should find the different DM group");
            assert_eq!(kept_different_dm.id, different_dm.id);

            // Verify regular group is kept
            let kept_regular = deduplicated_groups
                .iter()
                .find(|g| g.dm_id.is_none())
                .expect("Should find the regular group");
            assert_eq!(kept_regular.id, regular_group.id);

            // Test with include_duplicate_dms = true (no deduplication)
            let all_groups = conn
                .find_groups(GroupQueryArgs {
                    include_duplicate_dms: true,
                    ..Default::default()
                })
                .unwrap();

            // Should have all 5 groups
            assert_eq!(all_groups.len(), 5);
        })
        .await
    }
}
