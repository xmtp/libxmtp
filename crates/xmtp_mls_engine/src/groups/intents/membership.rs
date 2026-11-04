//! Membership intents: membership updates, their results, and installation re-adds.

use super::*;

#[derive(Debug, Default, Clone)]
pub struct UpdateGroupMembershipResult {
    pub added_members: HashMap<String, u64>,
    pub removed_members: Vec<String>,
    pub failed_installations: Vec<Vec<u8>>,
}

impl UpdateGroupMembershipResult {
    pub fn new(
        added_members: HashMap<String, u64>,
        removed_members: Vec<String>,
        failed_installations: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            added_members,
            removed_members,
            failed_installations,
        }
    }
}

impl From<UpdateGroupMembershipIntentData> for UpdateGroupMembershipResult {
    fn from(value: UpdateGroupMembershipIntentData) -> Self {
        UpdateGroupMembershipResult::new(
            value.membership_updates,
            value.removed_members,
            value.failed_installations,
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct UpdateGroupMembershipIntentData {
    pub membership_updates: HashMap<String, u64>,
    pub removed_members: Vec<String>,
    pub failed_installations: Vec<Vec<u8>>,
    /// Inboxes in `membership_updates` that were members when the intent was
    /// built and that the app did not ask to add. Empty on intents that
    /// earlier builds stored: each of their entries can insert its inbox.
    pub refreshed_members: Vec<String>,
    /// Add no leaf; record each installation the raise would add as failed.
    pub removal_only: bool,
    /// Removals made for leave requests. Empty on older intents.
    pub leave_removals: Vec<String>,
}

impl UpdateGroupMembershipIntentData {
    pub fn new(
        membership_updates: HashMap<String, u64>,
        removed_members: Vec<String>,
        failed_installations: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            membership_updates,
            removed_members,
            failed_installations,
            refreshed_members: Vec::new(),
            removal_only: false,
            leave_removals: Vec::new(),
        }
    }

    /// Mark these inboxes' entries as refreshes of existing members.
    pub fn with_refreshed_members(mut self, mut refreshed_members: Vec<String>) -> Self {
        // Encode the same intent to the same bytes.
        refreshed_members.sort_unstable();
        refreshed_members.dedup();
        self.refreshed_members = refreshed_members;
        self
    }

    pub fn with_leave_removals(mut self, leave_removals: Vec<String>) -> Self {
        self.leave_removals = leave_removals;
        self
    }

    // implements: GMOD-033
    /// Drop each leave removal whose request is no longer pending.
    pub fn settle_leave_removals(&mut self, pending: &HashSet<String>) {
        let leaves = &self.leave_removals;
        self.removed_members
            .retain(|i| !leaves.contains(i) || pending.contains(i));
        self.leave_removals.retain(|i| pending.contains(i));
    }

    pub fn is_empty(&self) -> bool {
        self.membership_updates.is_empty()
            && self.removed_members.is_empty()
            && self.failed_installations.is_empty()
    }

    pub fn apply_to_group_membership(&self, group_membership: &GroupMembership) -> GroupMembership {
        tracing::info!("old group membership: {:?}", group_membership.members);
        let mut new_membership = group_membership.clone();
        for (inbox_id, &sequence_id) in self.membership_updates.iter() {
            let sequence_id = match group_membership.get(inbox_id) {
                // A queued refresh can be older than a commit that raised this
                // entry. Keep the higher sequence id; never lower it.
                Some(&current) => current.max(sequence_id),
                // implements: GMOD-051
                // A refresh raises an existing entry. A commit may have removed
                // the inbox after the refresh was built; do not insert it again.
                None if self.refreshed_members.contains(inbox_id) => continue,
                None => sequence_id,
            };
            new_membership.add(inbox_id.clone(), sequence_id);
        }

        for inbox_id in self.removed_members.iter() {
            new_membership.remove(inbox_id)
        }

        new_membership.failed_installations = new_membership
            .failed_installations
            .into_iter()
            .chain(self.failed_installations.iter().cloned())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        tracing::info!("updated group membership: {:?}", new_membership.members);
        new_membership
    }
}

impl From<UpdateGroupMembershipIntentData> for Vec<u8> {
    fn from(intent: UpdateGroupMembershipIntentData) -> Self {
        let mut buf = Vec::new();

        UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(UpdateGroupMembershipV1 {
                membership_updates: intent.membership_updates,
                removed_members: intent.removed_members,
                failed_installations: intent.failed_installations,
                refreshed_members: intent.refreshed_members,
                removal_only: intent.removal_only,
                leave_removals: intent.leave_removals,
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }
}

impl TryFrom<Vec<u8>> for UpdateGroupMembershipIntentData {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        if let UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(v1)),
        } = UpdateGroupMembershipData::decode(data.as_slice())?
        {
            Ok(Self {
                membership_updates: v1.membership_updates,
                removed_members: v1.removed_members,
                failed_installations: v1.failed_installations,
                refreshed_members: v1.refreshed_members,
                removal_only: v1.removal_only,
                leave_removals: v1.leave_removals,
            })
        } else {
            Err(IntentError::MissingPayload)
        }
    }
}

impl<'a> TryFrom<&'a [u8]> for UpdateGroupMembershipIntentData {
    type Error = IntentError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if let UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(v1)),
        } = UpdateGroupMembershipData::decode(data)?
        {
            Ok(Self {
                membership_updates: v1.membership_updates,
                removed_members: v1.removed_members,
                failed_installations: v1.failed_installations,
                refreshed_members: v1.refreshed_members,
                removal_only: v1.removal_only,
                leave_removals: v1.leave_removals,
            })
        } else {
            Err(IntentError::MissingPayload)
        }
    }
}

pub(crate) struct ReaddInstallationsIntentData {
    pub readded_installations: Vec<Vec<u8>>,
}

impl ReaddInstallationsIntentData {
    pub fn new(readded_installations: Vec<Vec<u8>>) -> Self {
        Self {
            readded_installations,
        }
    }
}

impl From<ReaddInstallationsIntentData> for Vec<u8> {
    fn from(intent: ReaddInstallationsIntentData) -> Self {
        let mut buf = Vec::new();
        ReaddInstallationsData {
            version: Some(ReaddInstallationsVersion::V1(ReaddInstallationsV1 {
                readded_installations: intent.readded_installations,
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }
}

impl TryFrom<Vec<u8>> for ReaddInstallationsIntentData {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        if let ReaddInstallationsData {
            version: Some(ReaddInstallationsVersion::V1(v1)),
        } = ReaddInstallationsData::decode(data.as_slice())?
        {
            Ok(Self::new(v1.readded_installations))
        } else {
            Err(IntentError::MissingPayload)
        }
    }
}

impl TryFrom<&[u8]> for ReaddInstallationsIntentData {
    type Error = IntentError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        if let ReaddInstallationsData {
            version: Some(ReaddInstallationsVersion::V1(v1)),
        } = ReaddInstallationsData::decode(data)?
        {
            Ok(Self::new(v1.readded_installations))
        } else {
            Err(IntentError::MissingPayload)
        }
    }
}
