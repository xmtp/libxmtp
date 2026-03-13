use xmtp_db::identity_update::StoredIdentityUpdate;
use xmtp_id::associations::{AssociationError, unverified::UnverifiedIdentityUpdate};

/// Extension trait for Identity types defined in [xmtp_id]
/// mostly helpful to glue together different parts of the codebase without introducing unnecessary
/// dependencies in other crates
pub trait IdentityExt {
    fn to_unverified(self) -> Result<UnverifiedIdentityUpdate, AssociationError>;
}

impl IdentityExt for StoredIdentityUpdate {
    fn to_unverified(self) -> Result<UnverifiedIdentityUpdate, AssociationError> {
        Ok(UnverifiedIdentityUpdate::try_from(self.payload)?)
    }
}
