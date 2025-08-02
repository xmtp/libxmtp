use crate::context::XmtpSharedContext;
use crate::groups::validated_commit::extract_group_membership;
use crate::groups::{GroupError, filter_inbox_ids_needing_updates};
use crate::identity_updates::load_identity_updates;
use openmls::prelude::StagedWelcome;
use std::collections::HashSet;

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

        let mut expected_installation_ids = HashSet::<Vec<u8>>::new();

        let identity_updates = crate::identity_updates::IdentityUpdates::new(&self.context);
        let futures: Vec<_> = membership
            .members
            .iter()
            .map(|(inbox_id, sequence_id)| {
                identity_updates.get_association_state(&db, inbox_id, Some(*sequence_id as i64))
            })
            .collect();
        let results = futures::future::try_join_all(futures).await?;

        for association_state in results {
            expected_installation_ids.extend(association_state.installation_ids());
        }

        let actual_installation_ids: HashSet<Vec<u8>> = staged_welcome
            .public_group()
            .members()
            .map(|member| member.signature_key)
            .collect();

        // exclude failed installations
        expected_installation_ids.retain(|id| !membership.failed_installations.contains(id));

        if expected_installation_ids != actual_installation_ids {
            return Err(GroupError::InvalidGroupMembership);
        }

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
