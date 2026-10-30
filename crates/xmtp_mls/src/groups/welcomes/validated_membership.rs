use crate::context::XmtpSharedContext;
use crate::groups::{GroupError, GroupMembership};
use crate::identity::parse_credential;
use crate::identity_updates::{
    IdentityDependencyError, IdentityRequirement, InstallationDiffError, MissingProofs,
    resolve_identity_requirements,
};
use openmls::prelude::{BasicCredential, StagedWelcome};
use std::collections::{HashMap, HashSet};
use xmtp_common::snippet::Snippet;
use xmtp_db::DbQuery;
use xmtp_mls_validation::{
    commit::extract_group_membership_with_failures, group_membership::FailedInstallations,
};

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
    failed: FailedInstallations,
    /// Tree members as inbox IDs and installation signature keys.
    members: Vec<(String, Vec<u8>)>,
}

impl WelcomeMembership {
    /// Copy public data from a trial without retaining staged MLS state.
    pub(crate) fn from_staged(welcome: &StagedWelcome) -> Result<Self, GroupError> {
        let extensions = welcome.group_context().extensions();
        let (membership, failed) = extract_group_membership_with_failures(extensions)
            .map_err(|_| GroupError::InvalidWelcomeMetadata)?;
        let members = welcome
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
            failed,
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
    // implements: JOIN-050
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
    // implements: JOIN-051
    async fn check_initial_membership(
        &self,
        welcome: &WelcomeMembership,
    ) -> Result<(), GroupError> {
        resolve_identity_requirements(&self.context, &welcome.requirements().collect())
            .await
            .map_err(InstallationDiffError::from)?;
        self.check_verified_membership(welcome, &self.context.db())
    }

    /// Name every missing proof in one `Need`, so one resolution fetches them all.
    // implements: JOIN-052, JOIN-053
    fn check_verified_membership(
        &self,
        welcome: &WelcomeMembership,
        db: &impl DbQuery,
    ) -> Result<(), GroupError> {
        let mut expected_members = HashMap::<String, HashSet<Vec<u8>>>::new();
        let mut missing = MissingProofs::default();
        for requirement in welcome.requirements() {
            let Some(association_state) = missing
                .read(db, &requirement)
                .map_err(InstallationDiffError::from)?
            else {
                continue;
            };
            expected_members.insert(
                association_state.inbox_id().to_string(),
                HashSet::from_iter(association_state.installation_ids()),
            );
        }
        missing.check().map_err(InstallationDiffError::from)?;

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
        for (inbox, installation_set) in &expected_members {
            for remaining_installation_id in installation_set {
                if !welcome
                    .failed
                    .of(inbox)
                    .is_some_and(|keys| keys.contains(remaining_installation_id))
                {
                    tracing::error!(
                        installation_id = %remaining_installation_id.as_slice().snippet(),
                        "Installation ID in expected members not found in ratchet tree",
                    );
                    return Err(GroupError::InvalidGroupMembership);
                }
            }
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
            _welcome: &WelcomeMembership,
        ) -> Result<(), GroupError> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tester;
    use std::collections::BTreeMap;
    use xmtp_db::identity_update::QueryIdentityUpdates;
    use xmtp_mls_common::{app_data::migration::encode_group_membership_dict, inbox_id::InboxId};
    use xmtp_mls_validation::commit::membership_entries;
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1, Version},
    };

    // verifies: JOIN-053
    #[xmtp_common::test(unwrap_try = true)]
    async fn misattributed_failed_installation_is_rejected() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        tester!(caro, disable_workers);
        tester!(bo2, from: bo, disable_workers);
        for client in [&*alix, &*bo, &*caro] {
            client.inbox_state(true).await?;
        }
        let sequence = |client: &crate::utils::FullXmtpClient| -> u64 {
            client
                .context
                .db()
                .get_latest_sequence_id_for_inbox(client.inbox_id())
                .unwrap() as u64
        };
        let absent_id = bo2.context.installation_id().to_vec();
        let members = [&*alix, &*bo, &*caro]
            .into_iter()
            .map(|client| {
                (
                    client.inbox_id().to_string(),
                    client.context.installation_id().to_vec(),
                )
            })
            .collect::<Vec<_>>();
        let validator = InitialMembershipValidator::new(caro.context.clone());
        for owner in [alix.inbox_id(), bo.inbox_id()] {
            let entries = [&*alix, &*bo, &*caro]
                .into_iter()
                .map(|client| {
                    (
                        InboxId::from_hex(client.inbox_id()).unwrap(),
                        GroupMembershipEntry {
                            version: Some(Version::V1(V1 {
                                sequence_id: sequence(client),
                                failed_installations: if client.inbox_id() == owner {
                                    vec![absent_id.clone()]
                                } else {
                                    vec![]
                                },
                            })),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let (membership, failed) =
                membership_entries(&encode_group_membership_dict(&entries)?)?;
            let welcome = WelcomeMembership {
                membership,
                failed,
                members: members.clone(),
            };
            let result = validator.check_initial_membership(&welcome).await;
            if owner == bo.inbox_id() {
                result?;
            } else {
                assert!(matches!(result, Err(GroupError::InvalidGroupMembership)));
            }
        }
    }
}
