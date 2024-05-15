use diesel::prelude::*;
use prost::Message;
use xmtp_id::associations::{AssociationState, DeserializationError};
use xmtp_proto::xmtp::identity::associations::AssociationState as AssociationStateProto;

use super::schema::association_state;
use crate::{impl_fetch, impl_store_or_ignore, storage::StorageError, Fetch, StoreOrIgnore};

/// StoredIdentityUpdate holds a serialized IdentityUpdate record
#[derive(Insertable, Identifiable, Queryable, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = association_state)]
#[diesel(primary_key(inbox_id, sequence_id))]
pub struct StoredAssociationState {
    pub inbox_id: String,
    pub sequence_id: i64,
    pub state: Vec<u8>,
}
impl_fetch!(StoredAssociationState, association_state, (String, i64));
impl_store_or_ignore!(StoredAssociationState, association_state);

impl TryFrom<StoredAssociationState> for AssociationState {
    type Error = DeserializationError;

    fn try_from(stored_state: StoredAssociationState) -> Result<Self, Self::Error> {
        return AssociationStateProto::decode(stored_state.state.as_slice())?.try_into();
    }
}

impl StoredAssociationState {
    pub fn write_to_cache(
        conn: &super::db_connection::DbConnection,
        inbox_id: String,
        sequence_id: i64,
        state: AssociationState,
    ) -> Result<(), StorageError> {
        let state_proto: AssociationStateProto = state.into();
        StoredAssociationState {
            inbox_id,
            sequence_id,
            state: state_proto.encode_to_vec(),
        }
        .store_or_ignore(conn)
    }

    pub fn read_from_cache(
        conn: &super::db_connection::DbConnection,
        inbox_id: String,
        sequence_id: i64,
    ) -> Result<Option<AssociationState>, StorageError> {
        let stored_state: Option<StoredAssociationState> =
            conn.fetch(&(inbox_id.to_string(), sequence_id))?;

        stored_state
            .map(|stored_state| {
                stored_state
                    .try_into()
                    .map_err(|err: DeserializationError| {
                        StorageError::Deserialization(format!(
                            "Failed to deserialize stored association state: {err:?}"
                        ))
                    })
            })
            .transpose()
    }
}
