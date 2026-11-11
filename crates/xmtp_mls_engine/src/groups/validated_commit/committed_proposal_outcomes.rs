use super::*;
use crate::groups::app_data::accumulate_app_data_updates;
use crate::groups::test_support::{EnginePeer, EngineTestPeer};
use crate::prelude::*;
use openmls::messages::proposals::{AppDataUpdateOperation, AppDataUpdateProposal, ProposalType};
use openmls_traits::OpenMlsProvider;
use xmtp_mls_group_state::app_data::component_id::ComponentId;

/// verifies: GMOD-030
///
/// A staged commit whose `AppDataUpdate` proposals mix an Update and a
/// Remove, or repeat a Remove, for one component is a terminal rejection.
/// The commits carry no `GroupContextExtensions` proposal, so OpenMLS's
/// own list check returns early at the first Remove and stages them.
#[xmtp_common::test(unwrap_try = true)]
async fn committed_proposal_outcomes_reject_conflicting_component_operations() {
    let alix = EngineTestPeer::new().await;
    let conversation = alix.create_group(None, None)?;
    conversation
        .update_group_description("described".to_string())
        .await?;
    let provider = alix.context.mls_provider();
    let signer = &alix.context.identity().installation_keys;
    let mut group = OpenMlsGroup::load(
        alix.context.mls_storage(),
        &conversation.group_id.to_openmls(),
    )??;
    let description = ComponentId::GROUP_DESCRIPTION;
    assert!(read_from_app_data_dict(description, &group).is_some());

    // One operation is proposed by reference and one is inline: two
    // identical Removes proposed by reference would share a reference.
    let update = || AppDataUpdateOperation::Update(b"changed".to_vec().into());
    for (by_reference, inline) in [
        (
            update(),
            AppDataUpdateProposal::remove(description.as_u16()),
        ),
        (
            AppDataUpdateOperation::Remove,
            AppDataUpdateProposal::update(description.as_u16(), b"changed".to_vec()),
        ),
        (
            AppDataUpdateOperation::Remove,
            AppDataUpdateProposal::remove(description.as_u16()),
        ),
    ] {
        group.propose_app_data_update(&provider, signer, description.as_u16(), by_reference)?;
        let inline = Proposal::AppDataUpdate(Box::new(inline));
        let pending: Vec<_> = group
            .pending_proposals()
            .map(QueuedProposal::proposal)
            .chain([&inline])
            .filter_map(|proposal| match proposal {
                Proposal::AppDataUpdate(p) => Some((p.component_id(), p.operation().clone())),
                _ => None,
            })
            .collect();
        let updates =
            accumulate_app_data_updates(&group, pending.iter().map(|(id, op)| (*id, op)))?;
        let mut stage = crate::groups::mls_ext::commit_without_group_info(
            &mut group,
            provider.storage(),
            |builder| builder.consume_proposal_store(true).add_proposal(inline),
        )?;
        stage.with_app_data_dictionary_updates(updates);
        stage
            .build(provider.rand(), provider.crypto(), signer, |_| true)?
            .stage_commit(&provider)?;
        let staged = group.pending_commit()?;
        let kinds: Vec<_> = staged
            .queued_proposals()
            .map(|queued| queued.proposal().proposal_type())
            .collect();
        assert_eq!(kinds, [ProposalType::AppDataUpdate; 2]);

        let result = ValidatedCommit::from_staged_commit_local(
            &alix.context,
            &alix.context.db(),
            staged,
            group.own_leaf_index(),
            &group,
            u64::MAX,
        );
        let Err(error) = result else {
            panic!("conflicting component operations were accepted");
        };
        assert!(
            matches!(
                &error,
                CommitValidationError::Rule(CommitRuleError::InvalidAppDataUpdateList(id))
                    if *id == description
            ),
            "{error:?}"
        );
        assert!(error.is_safe_rejection());

        group.clear_pending_commit(provider.storage())?;
        group.clear_pending_proposals(provider.storage())?;
    }
}

/// verifies: GMOD-059
///
/// A `GroupContextExtensions` proposal is a terminal rejection both
/// standalone and inside a commit.
#[xmtp_common::test(unwrap_try = true)]
// A test drives an OpenMLS convenience commit; its GroupInfo never leaves the test.
#[allow(clippy::disallowed_methods)]
async fn committed_proposal_outcomes_reject_gce_proposal_and_commit() {
    let alix = EngineTestPeer::new().await;
    let conversation = alix.create_group(None, None)?;
    let provider = alix.context.mls_provider();
    let mut group = OpenMlsGroup::load(
        alix.context.mls_storage(),
        &conversation.group_id.to_openmls(),
    )??;
    let signer = &alix.context.identity().installation_keys;
    group.propose_group_context_extensions(&provider, group.extensions().clone(), signer)?;
    let (immutable, mutable) = read_committed_metadata(&group)?;
    let proposal = group.pending_proposals().next()?;
    let error = validate_proposal(
        proposal,
        &group,
        &PolicySet::default(),
        &immutable,
        &mutable,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        CommitValidationError::Rule(CommitRuleError::UnsupportedProposalType(
            ProposalType::GroupContextExtensions
        ))
    ));
    assert!(error.is_safe_rejection());
    group.commit_to_pending_proposals(&provider, signer)?;
    let staged = group.pending_commit()?;
    let error = ValidatedCommit::from_staged_commit_local(
        &alix.context,
        &alix.context.db(),
        staged,
        group.own_leaf_index(),
        &group,
        u64::MAX,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        CommitValidationError::Rule(CommitRuleError::UnsupportedProposalType(
            ProposalType::GroupContextExtensions
        ))
    ));
    assert!(error.is_safe_rejection());
}

/// verifies: GMOD-059
///
/// A standalone `GroupContextExtensions` or `PreSharedKey` proposal is a
/// terminal rejection on receipt, so it never holds the group head.
#[xmtp_common::test(unwrap_try = true)]
async fn standalone_unsupported_proposals_are_terminal() {
    use openmls::schedule::PreSharedKeyId;
    let alix = EngineTestPeer::new().await;
    let conversation = alix.create_group(None, None)?;
    let provider = alix.context.mls_provider();
    let mut group = OpenMlsGroup::load(
        alix.context.mls_storage(),
        &conversation.group_id.to_openmls(),
    )??;
    let signer = &alix.context.identity().installation_keys;
    group.propose_group_context_extensions(&provider, group.extensions().clone(), signer)?;
    group.propose_pre_shared_key(
        &provider,
        signer,
        PreSharedKeyId::external(b"psk".to_vec(), vec![0; 32]),
    )?;
    let (immutable, mutable) = read_committed_metadata(&group)?;
    let kinds: Vec<_> = group
        .pending_proposals()
        .map(|proposal| {
            let error = validate_proposal(
                proposal,
                &group,
                &PolicySet::default(),
                &immutable,
                &mutable,
            )
            .unwrap_err();
            assert!(error.is_safe_rejection(), "{error:?}");
            proposal.proposal().proposal_type()
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ProposalType::GroupContextExtensions,
            ProposalType::PreSharedKey
        ]
    );
}

/// verifies: GMOD-059
///
/// A queued `ReInit`, `ExternalInit`, `SelfRemove`, `AppEphemeral`, custom
/// or GREASE proposal is a terminal rejection under `validate_proposal`. Each is built from a real
/// queued proposal with its body replaced, because OpenMLS does not let a
/// member queue them through its API.
#[xmtp_common::test(unwrap_try = true)]
async fn validate_proposal_rejects_every_unsupported_type() {
    use openmls::messages::proposals::{
        AppEphemeralProposal, CustomProposal, ExternalInitProposal, ReInitProposal,
    };
    use tls_codec::DeserializeBytes as _;
    let alix = EngineTestPeer::new().await;
    let conversation = alix.create_group(None, None)?;
    let provider = alix.context.mls_provider();
    let mut group = OpenMlsGroup::load(
        alix.context.mls_storage(),
        &conversation.group_id.to_openmls(),
    )??;
    let signer = &alix.context.identity().installation_keys;
    group.propose_group_context_extensions(&provider, group.extensions().clone(), signer)?;
    let template = serde_json::to_value(group.pending_proposals().next()?)?;
    let (immutable, mutable) = read_committed_metadata(&group)?;
    // group_id <1 byte>, version mls10, ciphersuite 0x0001, no extensions.
    let reinit = ReInitProposal::tls_deserialize_exact_bytes(&[1, 7, 0, 1, 0, 1, 0])?;
    for proposal in [
        Proposal::ReInit(Box::new(reinit)),
        Proposal::ExternalInit(Box::new(ExternalInitProposal::from(vec![1]))),
        Proposal::SelfRemove,
        Proposal::AppEphemeral(Box::new(AppEphemeralProposal::new(0x8001, vec![1]))),
        Proposal::Custom(Box::new(CustomProposal::new(0xF000, vec![1]))),
        Proposal::Custom(Box::new(CustomProposal::new(0x0A0A, vec![1]))),
    ] {
        let kind = proposal.proposal_type();
        let mut queued = template.clone();
        queued["proposal"] = serde_json::to_value(&proposal)?;
        let queued: QueuedProposal = serde_json::from_value(queued)?;
        assert_eq!(queued.proposal().proposal_type(), kind);
        let Err(error) =
            validate_proposal(&queued, &group, &PolicySet::default(), &immutable, &mutable)
        else {
            panic!("{kind:?} was accepted");
        };
        assert!(
            matches!(
                &error,
                CommitValidationError::Rule(CommitRuleError::UnsupportedProposalType(found))
                    if *found == kind
            ),
            "{kind:?}: {error:?}"
        );
        assert!(error.is_safe_rejection(), "{kind:?}");
    }
}

/// A commit can carry thousands of proposals from one member. The proposal
/// changes resolve that member's leaf, credential and roles once, not once
/// per proposal.
#[xmtp_common::test(unwrap_try = true)]
async fn proposal_changes_resolve_each_proposer_once() {
    use xmtp_mls_validation::commit::count_participant_lookups;
    const PROPOSALS: usize = 2_000;
    let alix = EngineTestPeer::new().await;
    let conversation = alix.create_group(None, None)?;
    let provider = alix.context.mls_provider();
    let signer = &alix.context.identity().installation_keys;
    let mut group = OpenMlsGroup::load(
        alix.context.mls_storage(),
        &conversation.group_id.to_openmls(),
    )??;
    let name = ComponentId::GROUP_NAME.as_u16();
    let updates: Vec<_> = (0..PROPOSALS)
        .map(|n| AppDataUpdateProposal::update(name, format!("name {n}").into_bytes()))
        .collect();
    let dictionary = accumulate_app_data_updates(
        &group,
        updates
            .iter()
            .map(|update| (update.component_id(), update.operation())),
    )?;
    let proposals = updates
        .into_iter()
        .map(|update| Proposal::AppDataUpdate(Box::new(update)));
    let mut stage = crate::groups::mls_ext::commit_without_group_info(
        &mut group,
        provider.storage(),
        |builder| builder.add_proposals(proposals),
    )?;
    stage.with_app_data_dictionary_updates(dictionary);
    stage
        .build(provider.rand(), provider.crypto(), signer, |_| true)?
        .stage_commit(&provider)?;
    let staged = group.pending_commit()?;
    assert_eq!(staged.queued_proposals().count(), PROPOSALS);

    let (immutable, mutable) = read_committed_metadata(&group)?;
    let (changes, lookups) =
        count_participant_lookups(|| get_proposal_changes(staged, &group, &immutable, &mutable));
    changes?;
    assert_eq!(lookups, 1, "the proposer was resolved once per proposal");
}
