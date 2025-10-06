use crate::context::XmtpSharedContext;
use crate::groups::validated_commit::extract_group_membership;
use crate::groups::{GroupError, filter_inbox_ids_needing_updates};
use crate::identity::parse_credential;
use crate::identity_updates::load_identity_updates;
use openmls::prelude::{BasicCredential, StagedWelcome};
use std::collections::{HashMap, HashSet};

#[allow(async_fn_in_trait)]
pub trait ValidateGroupMembership {
    ///
    /// Ensures that the membership in the MLS tree matches the inboxes specified in the `GroupMembership` extension.
    ///
    async fn check_initial_membership(&self, welcome: &StagedWelcome) -> Result<(), GroupError>;
}

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
        staged_welcome: &StagedWelcome,
    ) -> Result<(), GroupError> {
        let db = self.context.db();
        tracing::info!("Validating initial group membership");
        let extensions = staged_welcome.public_group().group_context().extensions();
        let membership = extract_group_membership(extensions)?;
        let needs_update =
            filter_inbox_ids_needing_updates(&db, membership.to_filters().as_slice())?;
        if !needs_update.is_empty() {
            let ids = needs_update.iter().map(AsRef::as_ref).collect::<Vec<_>>();
            load_identity_updates(self.context.api(), &db, ids.as_slice()).await?;
        }

        let identity_updates = crate::identity_updates::IdentityUpdates::new(&self.context);
        let futures: Vec<_> = membership
            .members
            .iter()
            .map(|(inbox_id, sequence_id)| {
                identity_updates.get_association_state(&db, inbox_id, Some(*sequence_id as i64))
            })
            .collect();
        let results = futures::future::try_join_all(futures).await?;

        let mut expected_members = HashMap::<String, HashSet<Vec<u8>>>::new();
        for association_state in results {
            expected_members.insert(
                association_state.inbox_id().to_string(),
                HashSet::from_iter(association_state.installation_ids()),
            );
        }

        for member in staged_welcome.public_group().members() {
            let basic_credential = BasicCredential::try_from(member.credential.clone())?;
            let claimed_inbox_id = parse_credential(basic_credential.identity())?;
            let Some(installation_ids) = expected_members.get_mut(&claimed_inbox_id) else {
                tracing::error!(
                    claimed_inbox_id = claimed_inbox_id,
                    "Inbox ID not found in expected members",
                );
                return Err(GroupError::InvalidGroupMembership);
            };
            if !installation_ids.contains(&member.signature_key) {
                tracing::error!(
                    claimed_inbox_id = claimed_inbox_id,
                    installation_id = hex::encode(member.signature_key),
                    "Installation ID not found in expected members for inbox ID",
                );
                return Err(GroupError::InvalidGroupMembership);
            }
            installation_ids.remove(&member.signature_key);
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
            _welcome: &StagedWelcome,
        ) -> Result<(), GroupError> {
            Ok(())
        }
    }
}
