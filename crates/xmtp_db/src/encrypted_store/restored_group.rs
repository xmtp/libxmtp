//! Change detection for groups in the Restored state.
//!
//! A receive controller must not keep network interest in a Restored group.
//! A client that stores a Restored group notifies its own controller, but
//! another client that shares the database does not. Triggers raise the
//! `restored_group_generation` counter on every change of a group into the
//! Restored state, by any writer. A controller reads the counter on each pass
//! and, only after it changes, finds which of its selected groups are Restored.

use diesel::prelude::*;
use xmtp_proto::types::GroupId;

use super::schema::{groups, restored_group_generation as generation};
use crate::{ConnectionError, ConnectionExt, group::GroupMembershipState};

/// Rows per `IN` list, well under SQLite's bound-parameter limit.
const IDS_PER_QUERY: usize = 1000;

pub trait QueryRestoredGroups: ConnectionExt + Sized {
    /// Increases each time a group enters the Restored state.
    fn restored_group_generation(&self) -> Result<i64, ConnectionError> {
        self.raw_query(|conn| generation::table.select(generation::generation).first(conn))
    }

    /// The members of `group_ids` that are stored in the Restored state.
    fn restored_among(&self, group_ids: &[GroupId]) -> Result<Vec<GroupId>, ConnectionError> {
        let mut restored = Vec::new();
        for chunk in group_ids.chunks(IDS_PER_QUERY) {
            restored.extend(self.raw_query(|conn| {
                groups::table
                    .filter(groups::id.eq_any(chunk))
                    .filter(groups::membership_state.eq(GroupMembershipState::Restored))
                    .select(groups::id)
                    .load::<GroupId>(conn)
            })?);
        }
        Ok(restored)
    }
}

impl<C: ConnectionExt> QueryRestoredGroups for C {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Store, StoreOrIgnore,
        group::{QueryGroup, tests::generate_group},
        test_utils::with_connection,
    };

    // verifies: PROC-051
    #[xmtp_common::test(unwrap_try = true)]
    fn generation_counts_each_change_into_restored() {
        with_connection(|conn| {
            let generation = || conn.restored_group_generation().unwrap();
            assert_eq!(generation(), 0);
            generate_group(None).store(conn)?;
            assert_eq!(generation(), 0);

            let restored = generate_group(Some(GroupMembershipState::Restored));
            restored.store(conn)?;
            assert_eq!(generation(), 1);
            // An ignored duplicate insert is not a change.
            restored.store_or_ignore(conn)?;
            assert_eq!(generation(), 1);

            conn.update_group_membership(restored.id, GroupMembershipState::Allowed)?;
            assert_eq!(generation(), 1);
            conn.update_group_membership(restored.id, GroupMembershipState::Restored)?;
            assert_eq!(generation(), 2);
            conn.update_group_membership(restored.id, GroupMembershipState::Restored)?;
            assert_eq!(generation(), 2);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn restored_among_returns_only_selected_restored_groups() {
        with_connection(|conn| {
            let restored = generate_group(Some(GroupMembershipState::Restored));
            let unselected = generate_group(Some(GroupMembershipState::Restored));
            let allowed = generate_group(None);
            for group in [&restored, &unselected, &allowed] {
                group.store(conn)?;
            }
            let missing: GroupId = xmtp_common::Generate::generate();
            assert_eq!(
                conn.restored_among(&[restored.id, allowed.id, missing])?,
                vec![restored.id]
            );
            assert!(conn.restored_among(&[])?.is_empty());
        })
    }
}
