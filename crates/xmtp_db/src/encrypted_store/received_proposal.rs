//! Durable evidence of the proposals a group's ordered prefix delivered.
//!
//! Group processing records a proposal reference here in the same state
//! transaction that stores the proposal in the MLS proposal store. The two are
//! independent rows, so when a commit names a proposal the store lacks, this
//! table tells whether the prefix never carried it (a terminal rejection) or
//! local state lost it (held work). Evidence is kept only for the latest epoch
//! with proposals: a commit can only reference proposals from its own epoch.

use diesel::prelude::*;
use xmtp_proto::types::GroupId;

use super::{schema::received_proposals as received, stream_storage::stream_transaction};
use crate::{ConnectionExt, StorageError};

pub trait QueryReceivedProposal: ConnectionExt + Sized {
    /// Record a proposal applied at `epoch` and discard evidence from earlier epochs.
    fn record_received_proposal(
        &self,
        group_id: GroupId,
        epoch: i64,
        reference: &[u8],
    ) -> Result<(), StorageError> {
        stream_transaction(self, |conn| {
            diesel::delete(
                received::table
                    .filter(received::group_id.eq(group_id))
                    .filter(received::epoch.lt(epoch)),
            )
            .execute(conn)?;
            diesel::insert_or_ignore_into(received::table)
                .values((
                    received::group_id.eq(group_id),
                    received::epoch.eq(epoch),
                    received::proposal_ref.eq(reference),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    /// Forget a proposal that a protocol decision evicted. A later reference to it is absent.
    fn forget_received_proposal(
        &self,
        group_id: GroupId,
        epoch: i64,
        reference: &[u8],
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::delete(received::table.find((group_id, epoch, reference))).execute(conn)
        })?;
        Ok(())
    }

    /// References of every proposal the prefix delivered at `epoch`.
    fn received_proposals(
        &self,
        group_id: GroupId,
        epoch: i64,
    ) -> Result<Vec<Vec<u8>>, StorageError> {
        Ok(self.raw_query(|conn| {
            received::table
                .filter(received::group_id.eq(group_id))
                .filter(received::epoch.eq(epoch))
                .select(received::proposal_ref)
                .load(conn)
        })?)
    }
}

impl<C: ConnectionExt> QueryReceivedProposal for C {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Store, group::tests::generate_group, test_utils::with_connection};

    /// Evidence is scoped to one group and epoch, a later epoch discards the
    /// earlier one, and an evicted proposal no longer counts as received.
    #[xmtp_common::test(unwrap_try = true)]
    fn evidence_is_scoped_to_the_latest_epoch() {
        with_connection(|conn| {
            let group = generate_group(None);
            let other = generate_group(None);
            group.store(conn)?;
            other.store(conn)?;
            conn.record_received_proposal(group.id, 1, &[1])?;
            conn.record_received_proposal(group.id, 1, &[1])?;
            conn.record_received_proposal(group.id, 1, &[2])?;
            conn.record_received_proposal(other.id, 1, &[3])?;
            assert_eq!(
                conn.received_proposals(group.id, 1)?,
                vec![vec![1], vec![2]]
            );

            conn.forget_received_proposal(group.id, 1, &[1])?;
            assert_eq!(conn.received_proposals(group.id, 1)?, vec![vec![2]]);

            conn.record_received_proposal(group.id, 2, &[4])?;
            assert!(conn.received_proposals(group.id, 1)?.is_empty());
            assert_eq!(conn.received_proposals(group.id, 2)?, vec![vec![4]]);
            assert_eq!(conn.received_proposals(other.id, 1)?, vec![vec![3]]);
        })
    }
}
