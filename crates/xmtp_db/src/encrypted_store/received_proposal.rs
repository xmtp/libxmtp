//! Durable evidence of the proposals a group's ordered prefix delivered.
//!
//! Group processing records a proposal reference here in the same state
//! transaction that stores the proposal in the MLS proposal store. The two are
//! independent rows, so when a commit names a proposal the store lacks, this
//! table tells whether the prefix never carried it (a terminal rejection) or
//! local state lost it (held work). Evidence lives exactly as long as the
//! proposal store's contents: it is cleared whenever the store is, on every
//! merged commit and on a replacing Welcome.

use diesel::prelude::*;
use xmtp_proto::types::GroupId;

use super::schema::received_proposals as received;
use crate::{ConnectionExt, StorageError};

pub trait QueryReceivedProposal: ConnectionExt + Sized {
    /// Record a proposal the prefix delivered at `epoch`.
    fn record_received_proposal(
        &self,
        group_id: GroupId,
        epoch: i64,
        reference: &[u8],
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::insert_or_ignore_into(received::table)
                .values((
                    received::group_id.eq(group_id),
                    received::epoch.eq(epoch),
                    received::proposal_ref.eq(reference),
                ))
                .execute(conn)
        })?;
        Ok(())
    }

    /// Forget every proposal of `group_id`, in step with clearing its proposal store.
    fn forget_received_proposals(&self, group_id: GroupId) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::delete(received::table.filter(received::group_id.eq(group_id))).execute(conn)
        })?;
        Ok(())
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

    /// Evidence is scoped to one group and epoch, an evicted proposal no longer
    /// counts as received, and clearing a group leaves other groups intact.
    #[xmtp_common::test(unwrap_try = true)]
    fn evidence_is_scoped_to_group_and_epoch() {
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
            assert_eq!(conn.received_proposals(group.id, 1)?, vec![vec![2]]);
            assert_eq!(conn.received_proposals(group.id, 2)?, vec![vec![4]]);

            conn.forget_received_proposals(group.id)?;
            assert!(conn.received_proposals(group.id, 1)?.is_empty());
            assert!(conn.received_proposals(group.id, 2)?.is_empty());
            assert_eq!(conn.received_proposals(other.id, 1)?, vec![vec![3]]);
        })
    }
}
