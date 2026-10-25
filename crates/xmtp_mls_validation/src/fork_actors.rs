//! Fork-recovery roles derived from committed conversation metadata.

use std::collections::HashSet;

use openmls::group::MlsGroup as OpenMlsGroup;
use xmtp_mls_common::{
    group_metadata::{DmMembers, GroupMetadata},
    group_mutable_metadata::GroupMutableMetadata,
};
use xmtp_proto::types::ConversationType;

use crate::commit::{CommitRuleError, inbox_id_from_credential, read_committed_metadata};

/// Inboxes permitted to repair a conversation and publish its commit log.
pub enum ForkActors {
    Dm(DmMembers<String>),
    SuperAdmins(Vec<String>),
}

/// Which same-installation readd exceptions a committer may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReaddAuthority {
    None,
    DmParticipant,
    SuperAdmin,
}

impl ForkActors {
    pub fn new(immutable: &GroupMetadata, mutable: &GroupMutableMetadata) -> Self {
        if immutable.conversation_type == ConversationType::Dm
            && let Some(members) = &immutable.dm_members
        {
            return Self::Dm(members.clone());
        }
        Self::SuperAdmins(mutable.super_admin_list.clone())
    }

    pub fn from_group(group: &OpenMlsGroup) -> Result<Self, CommitRuleError> {
        let (immutable, mutable) = read_committed_metadata(group)?;
        Ok(Self::new(&immutable, &mutable))
    }

    pub fn permits(&self, inbox_id: &str) -> bool {
        match self {
            Self::Dm(members) => {
                members.member_one_inbox_id == inbox_id || members.member_two_inbox_id == inbox_id
            }
            Self::SuperAdmins(inboxes) => inboxes.iter().any(|inbox| inbox == inbox_id),
        }
    }

    /// Explicit recipients; the one-shot sender also includes its own inbox.
    pub fn request_targets(&self, own_inbox_id: &str) -> Vec<String> {
        match self {
            Self::Dm(members) => [&members.member_one_inbox_id, &members.member_two_inbox_id]
                .into_iter()
                .filter(|inbox| inbox.as_str() != own_inbox_id)
                .cloned()
                .collect(),
            Self::SuperAdmins(inboxes) => inboxes.clone(),
        }
    }

    pub fn readd_authority(&self, committer_inbox_id: &str) -> ReaddAuthority {
        if !self.permits(committer_inbox_id) {
            return ReaddAuthority::None;
        }
        match self {
            Self::Dm(_) => ReaddAuthority::DmParticipant,
            Self::SuperAdmins(_) => ReaddAuthority::SuperAdmin,
        }
    }

    pub fn publisher_keys(
        &self,
        group: &OpenMlsGroup,
    ) -> Result<HashSet<Vec<u8>>, CommitRuleError> {
        let mut keys = HashSet::new();
        for member in group.members() {
            if self.permits(&inbox_id_from_credential(&member.credential)?) {
                keys.insert(member.signature_key);
            }
        }
        Ok(keys)
    }
}
