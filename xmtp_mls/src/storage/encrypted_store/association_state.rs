use diesel::prelude::*;

use super::schema::association_state;
use crate::{impl_fetch, impl_store_or_ignore};

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

// impl TryFrom<StoredAssociationState> for AssociationState {
//     type Error = AssociationError;
//
//     fn try_from(state: StoredAssociationState) -> Result<Self, Self::Error> {
//         AssociationState::try_from(state.state)
//     }
// }
