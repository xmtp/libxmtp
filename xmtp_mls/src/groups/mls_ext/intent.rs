use crate::{
    client::XmtpMlsLocalContext,
    groups::{
        group_membership::GroupMembership, mls_sync::PublishIntentData,
        scoped_client::ScopedGroupClient, validated_commit::CommitValidationError, GroupError,
    },
};
use openmls::{
    group::MlsGroup,
    prelude::{Extension, Extensions, LeafNodeIndex, UnknownExtension},
};
use openmls_traits::{storage::StorageProvider, OpenMlsProvider};
use std::collections::HashSet;
use xmtp_common::RetryableError;
use xmtp_db::{sql_key_store::SqlKeyStoreError, XmtpOpenMlsProvider};

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

    fn removed_leaf_nodes(
        &mut self,
        removed_installations: &HashSet<Vec<u8>>,
    ) -> Vec<LeafNodeIndex>;
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

    fn removed_leaf_nodes(
        &mut self,
        removed_installations: &HashSet<Vec<u8>>,
    ) -> Vec<LeafNodeIndex> {
        self.members()
            .filter(|member| removed_installations.contains(&member.signature_key))
            .map(|member| member.index)
            .collect()
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

pub trait GroupIntent {
    async fn publish_data(
        &self,
        provider: &XmtpOpenMlsProvider,
        client: impl ScopedGroupClient,
        context: &XmtpMlsLocalContext,
        group: &mut MlsGroup,
        should_push: bool,
    ) -> Result<Option<PublishIntentData>, GroupError>;
}
