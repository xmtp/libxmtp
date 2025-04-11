use crate::{
    configuration::GROUP_MEMBERSHIP_EXTENSION_ID,
    groups::{
        group_membership::GroupMembership, mls_sync::GroupMessageProcessingError,
        validated_commit::CommitValidationError,
    },
    identity::parse_credential,
};
use openmls::{
    group::MlsGroup,
    prelude::{
        BasicCredential, Extension, Extensions, LeafNodeIndex, ProcessedMessage, Sender,
        UnknownExtension,
    },
};
use openmls_traits::{storage::StorageProvider, OpenMlsProvider};
use std::collections::HashSet;
use xmtp_common::RetryableError;
use xmtp_db::sql_key_store::SqlKeyStoreError;
use xmtp_id::InboxId;

#[derive(thiserror::Error, Debug)]
pub enum GroupExtError {
    #[error(transparent)]
    Storage(#[from] xmtp_db::StorageError),
    #[error(transparent)]
    SqlKeyStore(#[from] SqlKeyStoreError),
}

impl RetryableError for GroupExtError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Storage(s) => s.is_retryable(),
            Self::SqlKeyStore(s) => s.is_retryable(),
        }
    }
}

pub trait MlsGroupExt {
    /// Get and clear the pending commit in an openmls MlsGroup
    fn get_and_clear_pending_commit<P: OpenMlsProvider>(
        &mut self,
        provider: P,
    ) -> Result<Option<Vec<u8>>, GroupExtError>
    where
        GroupExtError: From<<<P as OpenMlsProvider>::StorageProvider as StorageProvider<1>>::Error>;

    fn removed_leaf_nodes(&self, removed_installations: &HashSet<Vec<u8>>) -> Vec<LeafNodeIndex>;

    /// Extracts the message sender, but does not do any validation to ensure that the
    /// installation_id is actually part of the inbox.
    fn extract_sender(
        &mut self,
        message: &ProcessedMessage,
        message_created_ns: u64,
    ) -> Result<(InboxId, Vec<u8>), GroupMessageProcessingError>;

    /// Get the group membership from a group
    fn membership(&self) -> Result<GroupMembership, CommitValidationError>;
}

impl MlsGroupExt for MlsGroup {
    fn get_and_clear_pending_commit<P: OpenMlsProvider>(
        &mut self,
        provider: P,
    ) -> Result<Option<Vec<u8>>, GroupExtError>
    where
        GroupExtError: From<<<P as OpenMlsProvider>::StorageProvider as StorageProvider<1>>::Error>,
    {
        let commit = self
            .pending_commit()
            .as_ref()
            .map(xmtp_db::db_serialize)
            .transpose()?;
        self.clear_pending_commit(provider.storage())?;
        Ok(commit)
    }

    fn removed_leaf_nodes(&self, removed_installations: &HashSet<Vec<u8>>) -> Vec<LeafNodeIndex> {
        self.members()
            .filter(|member| removed_installations.contains(&member.signature_key))
            .map(|member| member.index)
            .collect()
    }

    fn extract_sender(
        &mut self,
        message: &ProcessedMessage,
        message_created_ns: u64,
    ) -> Result<(InboxId, Vec<u8>), GroupMessageProcessingError> {
        if let Sender::Member(leaf_node_index) = message.sender() {
            if let Some(member) = self.member_at(*leaf_node_index) {
                if member.credential.eq(message.credential()) {
                    let basic_credential = BasicCredential::try_from(member.credential)?;
                    let sender_inbox_id = parse_credential(basic_credential.identity())?;
                    return Ok((sender_inbox_id, member.signature_key));
                }
            }
        }

        let basic_credential = BasicCredential::try_from(message.credential().clone())?;
        Err(GroupMessageProcessingError::InvalidSender {
            message_time_ns: message_created_ns,
            credential: basic_credential.identity().to_vec(),
        })
    }

    /// Get The XMTP group Membership from a Group
    fn membership(&self) -> Result<GroupMembership, CommitValidationError> {
        self.extensions().group_membership()
    }
}

pub trait MlsExtensionsExt {
    fn group_membership(&self) -> Result<GroupMembership, CommitValidationError>;
}

impl MlsExtensionsExt for Extensions {
    fn group_membership(&self) -> Result<GroupMembership, CommitValidationError> {
        for extension in self.iter() {
            if let Extension::Unknown(
                GROUP_MEMBERSHIP_EXTENSION_ID,
                UnknownExtension(group_membership),
            ) = extension
            {
                return Ok(GroupMembership::try_from(group_membership.clone())?);
            }
        }
        Err(CommitValidationError::MissingGroupMembership)
    }
}
