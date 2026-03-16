mod commit_log_storer;
mod decrypted_welcome;
mod mls_ext_welcome_pointee_encryption_aead_type;
mod mls_ext_wrapper_encryption;
mod reload;
mod welcome_wrapper;

pub(crate) use commit_log_storer::*;
pub(crate) use decrypted_welcome::*;
pub use mls_ext_welcome_pointee_encryption_aead_type::*;
pub use mls_ext_wrapper_encryption::*;
use openmls::group::MlsGroup;
pub use reload::*;
pub use welcome_wrapper::*;
use xmtp_mls_common::group_mutable_metadata::GroupMutableMetadata;

use crate::groups::{MetadataPermissionsError, mls_sync::GroupMessageProcessingError};

pub trait MutableMetadataMlsExt {
    fn mutable_metadata(&self) -> Result<GroupMutableMetadata, GroupMessageProcessingError>;
}

impl MutableMetadataMlsExt for MlsGroup {
    fn mutable_metadata(&self) -> Result<GroupMutableMetadata, GroupMessageProcessingError> {
        GroupMutableMetadata::try_from(self)
            .map_err(MetadataPermissionsError::from)
            .map_err(GroupMessageProcessingError::from)
    }
}
