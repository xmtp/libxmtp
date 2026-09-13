use crate::context::XmtpSharedContext;
use crate::groups::validated_commit::extract_group_membership;
use crate::groups::{GroupError, GroupMembership};
use crate::identity::parse_credential;
use crate::identity_updates::{
    IdentityDependencyError, IdentityRequirement, InstallationDiffError, require_association_state,
    resolve_identity_requirements,
};
use openmls::prelude::{BasicCredential, StagedWelcome};
use std::collections::{HashMap, HashSet};
use xmtp_db::DbQuery;

/// Validate public trial membership, then recheck exact proofs under the writer.
#[allow(async_fn_in_trait)]
pub trait ValidateGroupMembership {
    /// Resolve identity proofs and check that the tree matches the membership extension.
    async fn check_initial_membership(&self, welcome: &WelcomeMembership)
    -> Result<(), GroupError>;

    /// Recheck exact proofs against the database read under the state writer.
    fn check_verified_membership(
        &self,
        _welcome: &WelcomeMembership,
        _db: &impl DbQuery,
    ) -> Result<(), GroupError> {
        Ok(())
    }
}

/// Public membership data from a trial decode. It contains no mutable MLS state.
#[derive(Debug, PartialEq)]
pub struct WelcomeMembership {
    /// Authenticated inbox membership and exact identity sequence references.
    membership: GroupMembership,
    /// Tree members as inbox IDs and installation signature keys.
    members: Vec<(String, Vec<u8>)>,
}

impl WelcomeMembership {
    /// Copy public data from a trial without retaining staged MLS state.
    pub(crate) fn from_staged(welcome: &StagedWelcome) -> Result<Self, GroupError> {
        let extensions = welcome.public_group().group_context().extensions();
        let membership =
            extract_group_membership(extensions).map_err(|_| GroupError::InvalidWelcomeMetadata)?;
        let members = welcome
            .public_group()
            .members()
            .map(|member| {
                let credential = BasicCredential::try_from(member.credential.clone())?;
                Ok((
                    parse_credential(credential.identity())?,
                    member.signature_key,
                ))
            })
            .collect::<Result<_, GroupError>>()?;
        Ok(Self {
            membership,
            members,
        })
    }

    fn requirements(&self) -> impl Iterator<Item = IdentityRequirement> + '_ {
        self.membership
            .members
            .iter()
            .map(|(inbox_id, sequence_id)| IdentityRequirement {
                inbox_id: inbox_id.clone(),
                sequence_id: *sequence_id,
            })
    }

    /// Reject zero identity references and references at or after this Welcome.
    pub(crate) fn validate_sequences(&self, welcome_sequence: u64) -> Result<(), GroupError> {
        for requirement in self.requirements() {
            if requirement.sequence_id == 0 || requirement.sequence_id >= welcome_sequence {
                return Err(
                    InstallationDiffError::from(IdentityDependencyError::InvalidSequence(
                        requirement.sequence_id,
                    ))
                    .into(),
                );
            }
        }
        Ok(())
    }
}

/// Check every installation against the exact identity state named by the Welcome.
pub struct InitialMembershipValidator<C> {
    context: C,
}

impl<C> InitialMembershipValidator<C> {
    pub fn new(context: C) -> InitialMembershipValidator<C> {
        Self { context }
    }
}

impl<C> ValidateGroupMembership for InitialMembershipValidator<C>
where
    C: XmtpSharedContext,
{
    async fn check_initial_membership(
        &self,
        welcome: &WelcomeMembership,
    ) -> Result<(), GroupError> {
        for (_, result) in
            resolve_identity_requirements(&self.context, welcome.requirements()).await
        {
            result.map_err(InstallationDiffError::from)?;
        }
        self.check_verified_membership(welcome, &self.context.db())
    }

    fn check_verified_membership(
        &self,
        welcome: &WelcomeMembership,
        db: &impl DbQuery,
    ) -> Result<(), GroupError> {
        let membership = &welcome.membership;
        let mut expected_members = HashMap::<String, HashSet<Vec<u8>>>::new();
        for requirement in welcome.requirements() {
            let association_state =
                require_association_state(db, &requirement).map_err(InstallationDiffError::from)?;
            expected_members.insert(
                association_state.inbox_id().to_string(),
                HashSet::from_iter(association_state.installation_ids()),
            );
        }

        for (claimed_inbox_id, signature_key) in &welcome.members {
            let Some(installation_ids) = expected_members.get_mut(claimed_inbox_id) else {
                tracing::error!(
                    claimed_inbox_id = claimed_inbox_id,
                    "Inbox ID not found in expected members",
                );
                return Err(GroupError::InvalidGroupMembership);
            };
            if !installation_ids.contains(signature_key) {
                tracing::error!(
                    claimed_inbox_id = claimed_inbox_id,
                    "Installation ID not found in expected members for inbox ID",
                );
                return Err(GroupError::InvalidGroupMembership);
            }
            installation_ids.remove(signature_key);
        }
        for installation_set in expected_members.values() {
            for remaining_installation_id in installation_set {
                if !membership
                    .failed_installations
                    .contains(remaining_installation_id)
                {
                    tracing::error!(
                        installation_id = hex::encode(remaining_installation_id),
                        "Installation ID in expected members not found in ratchet tree",
                    );
                    return Err(GroupError::InvalidGroupMembership);
                }
            }
        }
        // TODO: Is it an error if there are 'failed installations' that are not in the expected members list?

        tracing::info!("Group membership validated");

        Ok(())
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub mod test {
    use super::*;

    #[derive(Default)]
    pub struct NoopValidator;

    impl ValidateGroupMembership for NoopValidator {
        async fn check_initial_membership(
            &self,
            _welcome: &WelcomeMembership,
        ) -> Result<(), GroupError> {
            Ok(())
        }
    }
}
