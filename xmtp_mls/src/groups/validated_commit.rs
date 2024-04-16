use std::collections::HashMap;

use openmls::{
    credentials::{errors::BasicCredentialError, BasicCredential, CredentialType},
    group::{QueuedAddProposal, QueuedRemoveProposal},
    prelude::{LeafNodeIndex, MlsGroup as OpenMlsGroup, Sender, StagedCommit},
};
use thiserror::Error;

use xmtp_proto::xmtp::mls::message_contents::{
    GroupMembershipChanges, MembershipChange as MembershipChangeProto,
};

use super::{
    group_metadata::{extract_group_metadata, GroupMetadata, GroupMetadataError},
    members::aggregate_member_list,
};

use crate::{
    identity::{Identity, IdentityError},
    types::Address,
    verified_key_package::{KeyPackageVerificationError, VerifiedKeyPackage},
};

#[derive(Debug, Error)]
pub enum CommitValidationError {
    // Sender of the proposal has an invalid credential
    #[error("Invalid actor credential")]
    InvalidActorCredential,
    // Subject of the proposal has an invalid credential
    #[error("Invalid subject credential")]
    InvalidSubjectCredential,
    // Not used yet, but seems obvious enough to include now
    #[error("Insufficient permissions")]
    InsufficientPermissions,
    // TODO: We will need to relax this once we support external joins
    #[error("Actor not a member of the group")]
    ActorNotMember,
    #[error("Subject not a member of the group")]
    SubjectDoesNotExist,
    // TODO: We may need to relax this later
    // Current behaviour is to error out if a Commit includes proposals from multiple actors
    #[error("Multiple actors in commit")]
    MultipleActors,
    #[error("Failed to get member list {0}")]
    ListMembers(String),
    #[error("Failed to parse group metadata: {0}")]
    GroupMetadata(#[from] GroupMetadataError),
    #[error("Failed to validate identity: {0}")]
    IdentityValidation(#[from] IdentityError),
    #[error("invalid application id")]
    InvalidApplicationId,
    #[error("Credential error")]
    CredentialError(#[from] BasicCredentialError),
}

// A participant in a commit. Could be the actor or the subject of a proposal
#[derive(Clone, Debug)]
pub struct CommitParticipant {
    pub account_address: Address,
    pub installation_id: Vec<u8>,
    pub is_creator: bool,
}

// An aggregation of all the installation_ids for a given membership change
#[derive(Clone, Debug)]
pub struct AggregatedMembershipChange {
    pub(crate) installation_ids: Vec<Vec<u8>>,
    pub(crate) account_address: Address,
    #[allow(dead_code)]
    pub(crate) is_creator: bool,
}

// Account information for Metadata Change used for validation
#[derive(Clone, Debug)]
pub struct MetadataChange {
    #[allow(dead_code)]
    pub(crate) account_address: Address,
    #[allow(dead_code)]
    pub(crate) is_creator: bool,
}

// A parsed and validated commit that we can apply permissions and rules to
#[derive(Clone, Debug)]
pub struct ValidatedCommit {
    pub(crate) actor: CommitParticipant,
    pub(crate) members_added: Vec<AggregatedMembershipChange>,
    pub(crate) members_removed: Vec<AggregatedMembershipChange>,
    pub(crate) installations_added: Vec<AggregatedMembershipChange>,
    pub(crate) installations_removed: Vec<AggregatedMembershipChange>,
}

impl ValidatedCommit {
    // Build a ValidatedCommit from a StagedCommit and OpenMlsGroup
    pub fn from_staged_commit(
        staged_commit: &StagedCommit,
        openmls_group: &OpenMlsGroup,
    ) -> Result<Option<Self>, CommitValidationError> {
        for cred in staged_commit.credentials_to_verify() {
            if cred.credential_type() != CredentialType::Basic {
                return Err(CommitValidationError::InvalidActorCredential);
            }
            // TODO: Validate the credential
        }
        // We don't allow commits with proposals sent from multiple people right now
        // We also don't allow commits from external members
        let leaf_index = ensure_single_actor(staged_commit)?;
        if leaf_index.is_none() {
            // If we can't find a leaf index, it's a self update.
            // Return None until the issue is resolved
            return Ok(None);
        }
        let group_metadata = extract_group_metadata(openmls_group)?;
        let actor = extract_actor(
            leaf_index.expect("already checked"),
            openmls_group,
            &group_metadata,
        )?;

        let existing_members = aggregate_member_list(openmls_group)
            .map_err(|e| CommitValidationError::ListMembers(e.to_string()))?;

        let existing_installation_ids: HashMap<String, Vec<Vec<u8>>> = existing_members
            .into_iter()
            .fold(HashMap::new(), |mut acc, curr| {
                acc.insert(curr.account_address, curr.installation_ids);
                acc
            });

        let (members_added, installations_added) =
            get_new_members(staged_commit, &existing_installation_ids, &group_metadata)?;

        let (members_removed, installations_removed) = get_removed_members(
            staged_commit,
            &existing_installation_ids,
            openmls_group,
            &group_metadata,
        )?;

        let validated_commit = Self {
            actor,
            members_added,
            members_removed,
            installations_added,
            installations_removed,
        };

        if !group_metadata.policies.evaluate_commit(&validated_commit) {
            return Err(CommitValidationError::InsufficientPermissions);
        }

        Ok(Some(validated_commit))
    }

    pub fn actor_account_address(&self) -> Address {
        self.actor.account_address.clone()
    }

    pub fn actor_installation_id(&self) -> Vec<u8> {
        self.actor.installation_id.clone()
    }
}

impl AggregatedMembershipChange {
    pub fn to_proto(&self, initiated_by_account_address: Address) -> MembershipChangeProto {
        MembershipChangeProto {
            account_address: self.account_address.clone(),
            installation_ids: self.installation_ids.clone(),
            initiated_by_account_address,
        }
    }
}

fn extract_actor(
    leaf_index: LeafNodeIndex,
    group: &OpenMlsGroup,
    group_metadata: &GroupMetadata,
) -> Result<CommitParticipant, CommitValidationError> {
    if let Some(leaf_node) = group.member_at(leaf_index) {
        let signature_key = leaf_node.signature_key.as_slice();

        let basic_credential = BasicCredential::try_from(&leaf_node.credential)?;
        let account_address =
            Identity::get_validated_account_address(basic_credential.identity(), signature_key)?;

        let is_creator = account_address.eq(&group_metadata.creator_account_address);

        Ok(CommitParticipant {
            account_address,
            installation_id: signature_key.to_vec(),
            is_creator,
        })
    } else {
        // TODO: Handle external joins/commits
        Err(CommitValidationError::ActorNotMember)
    }
}

// Take a QueuedAddProposal and extract the wallet address and installation_id
fn extract_identity_from_add(
    proposal: QueuedAddProposal,
    group_metadata: &GroupMetadata,
) -> Result<CommitParticipant, CommitValidationError> {
    let key_package = proposal.add_proposal().key_package().to_owned();
    let verified_key_package =
        VerifiedKeyPackage::from_key_package(key_package).map_err(|e| match e {
            KeyPackageVerificationError::InvalidApplicationId => {
                CommitValidationError::InvalidApplicationId
            }
            _ => CommitValidationError::InvalidSubjectCredential,
        })?;

    let account_address = verified_key_package.account_address.clone();
    let is_creator = account_address.eq(&group_metadata.creator_account_address);

    Ok(CommitParticipant {
        account_address,
        installation_id: verified_key_package.installation_id(),
        is_creator,
    })
}

// Take a QueuedRemoveProposal and extract the wallet address and installation_id
fn extract_identity_from_remove(
    proposal: QueuedRemoveProposal,
    group: &OpenMlsGroup,
    group_metadata: &GroupMetadata,
) -> Result<CommitParticipant, CommitValidationError> {
    let leaf_index = proposal.remove_proposal().removed();

    if let Some(member) = group.member_at(leaf_index) {
        let signature_key = member.signature_key.as_slice();

        let basic_credential = BasicCredential::try_from(&member.credential)?;
        let account_address =
            Identity::get_validated_account_address(basic_credential.identity(), signature_key)?;
        let is_creator = account_address.eq(&group_metadata.creator_account_address);

        Ok(CommitParticipant {
            account_address,
            installation_id: signature_key.to_vec(),
            is_creator,
        })
    } else {
        Err(CommitValidationError::SubjectDoesNotExist)
    }
}

// Reducer function for merging members into a map, with all installation_ids collected per member
fn merge_members(
    mut acc: HashMap<String, AggregatedMembershipChange>,
    participant: CommitParticipant,
) -> HashMap<String, AggregatedMembershipChange> {
    acc.entry(participant.account_address.clone())
        .and_modify(|entry| {
            entry
                .installation_ids
                .push(participant.installation_id.clone())
        })
        .or_insert(AggregatedMembershipChange {
            account_address: participant.account_address,
            installation_ids: vec![participant.installation_id],
            is_creator: participant.is_creator,
        });
    acc
}

fn ensure_single_actor(
    staged_commit: &StagedCommit,
) -> Result<Option<LeafNodeIndex>, CommitValidationError> {
    let mut leaf_index: Option<&LeafNodeIndex> = None;
    for proposal in staged_commit.queued_proposals() {
        match proposal.sender() {
            Sender::Member(member_leaf_node_index) => {
                if leaf_index.is_none() {
                    leaf_index = Some(member_leaf_node_index);
                } else if !leaf_index.unwrap().eq(member_leaf_node_index) {
                    return Err(CommitValidationError::MultipleActors);
                }
            }
            _ => return Err(CommitValidationError::ActorNotMember),
        }
    }

    // Self updates don't produce any proposals I can see, so it will actually return
    // None in that case.
    // TODO: Figure out how to get the leaf index for self updates
    Ok(leaf_index.copied())
}

// Get a tuple of (new_members, new_installations), each formatted as a Member object with all installation_ids grouped
fn get_new_members(
    staged_commit: &StagedCommit,
    existing_installation_ids: &HashMap<String, Vec<Vec<u8>>>,
    group_metadata: &GroupMetadata,
) -> Result<
    (
        Vec<AggregatedMembershipChange>,
        Vec<AggregatedMembershipChange>,
    ),
    CommitValidationError,
> {
    let extracted_installs: Vec<CommitParticipant> = staged_commit
        .add_proposals()
        .map(|proposal| extract_identity_from_add(proposal, group_metadata))
        .collect::<Result<Vec<CommitParticipant>, CommitValidationError>>()?;

    let new_installations = extracted_installs
        .into_iter()
        .fold(HashMap::new(), merge_members);

    // Partition the list. If no existing member found, it is a new member. Otherwise it is just new installations
    Ok(new_installations
        .into_values()
        .partition(|member| !existing_installation_ids.contains_key(&member.account_address)))
}

// Get a tuple of (removed_members, removed_installations)
fn get_removed_members(
    staged_commit: &StagedCommit,
    existing_installation_ids: &HashMap<String, Vec<Vec<u8>>>,
    openmls_group: &OpenMlsGroup,
    group_metadata: &GroupMetadata,
) -> Result<
    (
        Vec<AggregatedMembershipChange>,
        Vec<AggregatedMembershipChange>,
    ),
    CommitValidationError,
> {
    let extracted_installs = staged_commit
        .remove_proposals()
        .map(|proposal| extract_identity_from_remove(proposal, openmls_group, group_metadata))
        .collect::<Result<Vec<CommitParticipant>, CommitValidationError>>()?;

    let removed_installations = extracted_installs
        .into_iter()
        .fold(HashMap::new(), merge_members);

    // Separate the fully removed members (where all installation ids were removed in the commit) from partial removals
    Ok(removed_installations.into_values().partition(|member| {
        match existing_installation_ids.get(&member.account_address) {
            Some(entry) => entry.len() == member.installation_ids.len(),
            None => true,
        }
    }))
}

impl From<ValidatedCommit> for GroupMembershipChanges {
    fn from(commit: ValidatedCommit) -> Self {
        let to_proto = |member: AggregatedMembershipChange| {
            member.to_proto(commit.actor.account_address.clone())
        };

        GroupMembershipChanges {
            members_added: commit.members_added.into_iter().map(to_proto).collect(),
            members_removed: commit.members_removed.into_iter().map(to_proto).collect(),
            installations_added: commit
                .installations_added
                .into_iter()
                .map(to_proto)
                .collect(),
            installations_removed: commit
                .installations_removed
                .into_iter()
                .map(to_proto)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use openmls::{
        credentials::{BasicCredential, CredentialWithKey},
        extensions::ExtensionType,
        group::config::CryptoConfig,
        messages::proposals::ProposalType,
        prelude::Capabilities,
        prelude_test::KeyPackage,
        versions::ProtocolVersion,
    };
    use xmtp_api_grpc::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;

    use super::ValidatedCommit;
    use crate::{
        builder::ClientBuilder,
        configuration::{CIPHERSUITE, MUTABLE_METADATA_EXTENSION_ID},
        Client,
    };

    fn get_key_package(client: &Client<GrpcClient>) -> KeyPackage {
        client
            .identity
            .new_key_package(&client.mls_provider(&client.store.conn().unwrap()))
            .unwrap()
    }

    #[tokio::test]
    async fn test_membership_changes() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_key_package = get_key_package(&bola);

        let amal_group = amal.create_group(None).unwrap();
        let amal_conn = amal.store.conn().unwrap();
        let amal_provider = amal.mls_provider(&amal_conn);
        let mut mls_group = amal_group.load_mls_group(&amal_provider).unwrap();
        // Create a pending commit to add bola to the group
        mls_group
            .add_members(
                &amal_provider,
                &amal.identity.installation_keys,
                &[bola_key_package],
            )
            .unwrap();

        let mut staged_commit = mls_group.pending_commit().unwrap();

        let message = ValidatedCommit::from_staged_commit(staged_commit, &mls_group)
            .unwrap()
            .unwrap();

        assert_eq!(message.installations_added.len(), 0);
        assert_eq!(message.members_added.len(), 1);
        assert_eq!(
            message.members_added[0].account_address,
            bola.account_address()
        );
        // Amal is the creator of the group and the actor
        assert!(message.actor.is_creator);
        // Bola is not the creator of the group
        assert!(!message.members_added[0].is_creator);

        // Merge the commit adding bola
        mls_group.merge_pending_commit(&amal_provider).unwrap();
        // Now we are going to remove bola

        let bola_leaf_node = mls_group
            .members()
            .find(|m| {
                m.signature_key
                    .eq(&bola.identity.installation_keys.public())
            })
            .unwrap()
            .index;
        mls_group
            .remove_members(
                &amal_provider,
                &amal.identity.installation_keys,
                &[bola_leaf_node],
            )
            .unwrap();

        staged_commit = mls_group.pending_commit().unwrap();
        let remove_message = ValidatedCommit::from_staged_commit(staged_commit, &mls_group)
            .unwrap()
            .unwrap();

        assert_eq!(remove_message.members_removed.len(), 1);
        assert_eq!(remove_message.installations_removed.len(), 0);
    }

    #[tokio::test]
    async fn test_installation_changes() {
        let wallet = generate_local_wallet();
        let amal_1 = ClientBuilder::new_test_client(&wallet).await;
        let amal_2 = ClientBuilder::new_test_client(&wallet).await;

        let amal_1_conn = amal_1.store.conn().unwrap();
        let amal_2_conn = amal_2.store.conn().unwrap();

        let amal_1_provider = amal_1.mls_provider(&amal_1_conn);
        let amal_2_provider = amal_2.mls_provider(&amal_2_conn);

        let amal_group = amal_1.create_group(None).unwrap();
        let mut amal_mls_group = amal_group.load_mls_group(&amal_1_provider).unwrap();

        let amal_2_kp = amal_2.identity.new_key_package(&amal_2_provider).unwrap();

        // Add Amal's second installation to the existing group
        amal_mls_group
            .add_members(
                &amal_1_provider,
                &amal_1.identity.installation_keys,
                &[amal_2_kp],
            )
            .unwrap();

        let staged_commit = amal_mls_group.pending_commit().unwrap();

        let validated_commit = ValidatedCommit::from_staged_commit(staged_commit, &amal_mls_group)
            .unwrap()
            .unwrap();

        assert_eq!(validated_commit.installations_added.len(), 1);
        assert_eq!(
            validated_commit.installations_added[0].installation_ids[0],
            amal_2.installation_public_key()
        )
    }

    #[tokio::test]
    async fn test_bad_key_package() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_conn = amal.store.conn().unwrap();
        let bola_conn = bola.store.conn().unwrap();

        let amal_provider = amal.mls_provider(&amal_conn);
        let bola_provider = bola.mls_provider(&bola_conn);

        let amal_group = amal.create_group(None).unwrap();
        let mut amal_mls_group = amal_group.load_mls_group(&amal_provider).unwrap();

        let capabilities = Capabilities::new(
            None,
            Some(&[CIPHERSUITE]),
            Some(&[
                ExtensionType::LastResort,
                ExtensionType::ApplicationId,
                ExtensionType::Unknown(MUTABLE_METADATA_EXTENSION_ID),
                ExtensionType::ImmutableMetadata,
            ]),
            Some(&[ProposalType::GroupContextExtensions]),
            None,
        );

        // Create a key package with a malformed credential
        let bad_key_package = KeyPackage::builder()
            .leaf_node_capabilities(capabilities)
            .build(
                CryptoConfig {
                    ciphersuite: CIPHERSUITE,
                    version: ProtocolVersion::default(),
                },
                &bola_provider,
                &bola.identity.installation_keys,
                CredentialWithKey {
                    // Broken credential
                    credential: BasicCredential::new(vec![1, 2, 3]).unwrap().into(),
                    signature_key: bola.identity.installation_keys.to_public_vec().into(),
                },
            )
            .unwrap();

        amal_mls_group
            .add_members(
                &amal_provider,
                &amal.identity.installation_keys,
                &[bad_key_package],
            )
            .unwrap();

        let staged_commit = amal_mls_group.pending_commit().unwrap();

        let validated_commit = ValidatedCommit::from_staged_commit(staged_commit, &amal_mls_group);

        assert!(validated_commit.is_err());
    }
}
