use crate::icebox::{Icebox, IceboxDependency};
use diesel::QueryableByName;
use xmtp_proto::types::OrphanedEnvelope;

/// Internal struct for flat query results before grouping
#[derive(Debug, QueryableByName)]
pub(super) struct IceboxWithDep {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub sequence_id: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub originator_id: i64,
    #[diesel(sql_type = diesel::sql_types::Binary)]
    group_id: *const [u8],
    #[diesel(sql_type = diesel::sql_types::Binary)]
    envelope_payload: *const [u8],
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub dependency_originator_id: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub dependency_sequence_id: i64,
}

impl IceboxWithDep {
    pub(super) unsafe fn group_id(&self) -> Vec<u8> {
        let slice_ptr = unsafe { &*self.group_id };
        slice_ptr.to_owned()
    }

    pub(super) unsafe fn envelope_payload(&self) -> Vec<u8> {
        let slice_ptr = unsafe { &*self.envelope_payload };
        slice_ptr.to_owned()
    }
}

// From<> impl not possible b/c of orphan rules
pub(super) trait IceboxOrphans {
    fn deps(&self) -> Vec<IceboxDependency>;
}

impl IceboxOrphans for OrphanedEnvelope {
    fn deps(&self) -> Vec<IceboxDependency> {
        let cursor = self.cursor;
        self.depends_on
            .iter()
            .map(|(oid, sid)| IceboxDependency {
                envelope_sequence_id: cursor.sequence_id as i64,
                envelope_originator_id: cursor.originator_id as i64,
                dependency_sequence_id: *sid as i64,
                dependency_originator_id: *oid as i64,
            })
            .collect()
    }
}

impl From<OrphanedEnvelope> for Icebox {
    fn from(value: OrphanedEnvelope) -> Self {
        Icebox {
            sequence_id: value.cursor.sequence_id as i64,
            originator_id: value.cursor.originator_id as i64,
            group_id: value.group_id.to_vec(),
            envelope_payload: value.into_payload().to_vec(),
        }
    }
}
