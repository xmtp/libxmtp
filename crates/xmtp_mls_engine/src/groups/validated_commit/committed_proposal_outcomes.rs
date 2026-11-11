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

/// Stage, as a receiver does, a commit by `group`'s own member whose
/// proposal list is exactly `proposals`, repeats included. A commit builder
/// keeps one of each proposal, as a modified client need not, so the commit
/// is framed by hand. The group's public view stages it: it checks the
/// signature, but not the confirmation tag, which only a key schedule over
/// such a list could compute. Its dictionary applies the whole list in
/// order, as receive staging does.
fn stage_listed_commit(
    group: &OpenMlsGroup,
    provider: &impl OpenMlsProvider,
    signer: &impl openmls_traits::signatures::Signer,
    proposals: &[AppDataUpdateProposal],
) -> StagedCommit {
    use openmls::{
        framing::{MlsMessageIn, ProcessedMessageContent},
        test_utils::frankenstein::{
            FrankenCommit, FrankenFramedContent, FrankenFramedContentAuthData,
            FrankenFramedContentBody, FrankenMlsMessage, FrankenMlsMessageBody, FrankenProposal,
            FrankenProposalOrRef, FrankenPublicMessage, FrankenSender, sign_with_label,
        },
    };
    use tls_codec::{Deserialize as _, Serialize as _};

    let proposals = proposals
        .iter()
        .map(|proposal| {
            let proposal = Proposal::AppDataUpdate(Box::new(proposal.clone()));
            let bytes = proposal.tls_serialize_detached().unwrap();
            FrankenProposalOrRef::Proposal(FrankenProposal::tls_deserialize_exact(bytes).unwrap())
        })
        .collect();
    let content = FrankenFramedContent {
        group_id: group.group_id().as_slice().to_vec().into(),
        epoch: group.epoch().as_u64(),
        sender: FrankenSender::Member(group.own_leaf_index().u32()),
        authenticated_data: Vec::new().into(),
        body: FrankenFramedContentBody::Commit(FrankenCommit {
            proposals,
            path: None,
        }),
    };
    // The FramedContentTBS of a member's PublicMessage (wire format 1).
    let mut tbs = [1u16.to_be_bytes(), 1u16.to_be_bytes()].concat();
    tbs.extend(content.tls_serialize_detached().unwrap());
    tbs.extend(
        group
            .export_group_context()
            .tls_serialize_detached()
            .unwrap(),
    );
    let auth = FrankenFramedContentAuthData {
        signature: sign_with_label(signer, b"FramedContentTBS", &tbs).into(),
        confirmation_tag: Some(vec![0; 32].into()),
    };
    let message = FrankenMlsMessage {
        version: 1,
        body: FrankenMlsMessageBody::PublicMessage(FrankenPublicMessage {
            content,
            auth,
            membership_tag: Some(vec![0; 32].into()),
        }),
    };
    let message =
        MlsMessageIn::tls_deserialize_exact(message.tls_serialize_detached().unwrap()).unwrap();
    let public = group.public_group();
    let processed = public
        .process_message(
            provider.crypto(),
            message.try_into_protocol_message().unwrap(),
        )
        .unwrap();
    let ProcessedMessageContent::UnresolvedAppDataCommit(unresolved) = processed.into_content()
    else {
        panic!("the commit carries no AppDataUpdate proposal");
    };
    let listed: Vec<_> = unresolved
        .app_data_update_proposals()
        .map(|proposal| (proposal.component_id(), proposal.operation().clone()))
        .collect();
    let updates =
        accumulate_app_data_updates(group, listed.iter().map(|(id, op)| (*id, op))).unwrap();
    public
        .stage_app_data_commit(provider.crypto(), *unresolved, updates)
        .unwrap()
}

/// A registry delta of `delta` as an inline proposal.
fn registry_update(
    delta: xmtp_mls_tls_collections::tls_map::TlsMapDelta<ComponentId, tls_codec::VLBytes>,
) -> AppDataUpdateProposal {
    use tls_codec::Serialize as _;
    AppDataUpdateProposal::update(
        ComponentId::COMPONENT_REGISTRY.as_u16(),
        delta.tls_serialize_detached().unwrap(),
    )
}

/// A commit can list one proposal twice. OpenMLS stages its dictionary from
/// the whole list but queues the proposal once, and validation judges the
/// queue. Release clients judge the registry the queued proposals leave,
/// not the staged one, so this client does too: here the queue deletes the
/// membership's registry entry and inserts it back, which leaves a valid
/// registry, while the list deletes it again.
#[xmtp_common::test(unwrap_try = true)]
async fn repeated_proposal_is_judged_on_the_validated_registry() {
    use crate::groups::app_data::load_component_registry;
    use prost::Message as _;
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;

    let alix = EngineTestPeer::new().await;
    let conversation = alix.create_group(None, None)?;
    let provider = alix.context.mls_provider();
    let signer = &alix.context.identity().installation_keys;
    let group = OpenMlsGroup::load(
        alix.context.mls_storage(),
        &conversation.group_id.to_openmls(),
    )??;
    let registry = load_component_registry(&group)?;
    let membership = ComponentId::GROUP_MEMBERSHIP;
    let entry = registry.get(&membership)??;
    let delete = registry_update(TlsMapDelta::new().delete(membership));
    let insert =
        registry_update(TlsMapDelta::new().insert(membership, entry.encode_to_vec().into()));

    let staged = stage_listed_commit(&group, &provider, signer, &[delete.clone(), insert, delete]);
    assert_eq!(staged.queued_proposals().count(), 2);
    let staged_registry = xmtp_mls_group_state::app_data::component_registry::ComponentRegistry::from_bytes(
        xmtp_mls_group_state::app_data::component_source::read_from_app_data_dict_from_extensions(
            ComponentId::COMPONENT_REGISTRY,
            staged.group_context().extensions(),
        )?,
    )?;
    assert!(
        staged_registry.get(&membership)?.is_none(),
        "the staged registry keeps the entry"
    );

    let (immutable, mutable) = read_committed_metadata(&group)?;
    let post_states = validate_app_data_update_proposals_in_commit(
        &staged, &group, &immutable, &mutable, &registry,
    )?;
    let post_registry = post_states[&ComponentId::COMPONENT_REGISTRY].as_deref()?;
    assert_eq!(
        Some(post_registry.as_slice()),
        read_from_app_data_dict(ComponentId::COMPONENT_REGISTRY, &group)
    );
}

/// The protocol floor and the metadata changes of a commit that lists one
/// proposal twice come from the queued proposals, as release clients read
/// them. Queued, the floor proposals raise the floor above this client's
/// version, so it pauses, while the list lowers it again. Queued, the name
/// proposals change the name, while the list sets it back.
#[xmtp_common::test(unwrap_try = true)]
async fn repeated_proposal_floor_and_metadata_come_from_the_queue() {
    use xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION;

    let alix = EngineTestPeer::new().await;
    let conversation = alix.create_group(None, None)?;
    conversation.update_group_name("x".to_string()).await?;
    let provider = alix.context.mls_provider();
    let signer = &alix.context.identity().installation_keys;
    let group = OpenMlsGroup::load(
        alix.context.mls_storage(),
        &conversation.group_id.to_openmls(),
    )??;
    let validate = |staged: &StagedCommit| {
        ValidatedCommit::from_staged_commit_local(
            &alix.context,
            &alix.context.db(),
            staged,
            group.own_leaf_index(),
            &group,
            u64::MAX,
        )
    };

    let floor = ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16();
    let current =
        AppDataUpdateProposal::update(floor, PROPOSALS_MIN_PROTOCOL_VERSION.as_bytes().to_vec());
    let above = AppDataUpdateProposal::update(floor, b"99.0.0".to_vec());
    let staged = stage_listed_commit(
        &group,
        &provider,
        signer,
        &[current.clone(), above, current],
    );
    let error = validate(&staged).unwrap_err();
    assert!(
        matches!(
            &error,
            CommitValidationError::Rule(CommitRuleError::ProtocolVersionTooLow(floor))
                if floor == "99.0.0"
        ),
        "{error:?}"
    );

    let name = ComponentId::GROUP_NAME;
    let set = |value: &str| AppDataUpdateProposal::update(name.as_u16(), value.as_bytes().to_vec());
    let staged = stage_listed_commit(&group, &provider, signer, &[set("x"), set("y"), set("x")]);
    let validated = validate(&staged)?;
    assert_eq!(validated.metadata_component_ids, [name.as_u16()]);
    let renamed: Vec<_> = validated
        .metadata_changes
        .metadata_field_changes
        .iter()
        .map(|change| (change.old_value.as_deref(), change.new_value.as_deref()))
        .collect();
    assert_eq!(renamed, [(Some("x"), Some("y"))]);
}

/// The self-or-non-member policy tests a deleted key against the membership
/// that the commit's queued proposals leave, as release clients do. Queued,
/// the membership proposals add an inbox and take it out again, so another
/// member may delete that inbox's entry of a self-owned map; the list adds
/// the inbox back.
#[xmtp_common::test(unwrap_try = true)]
async fn repeated_proposal_membership_comes_from_the_queue() {
    use crate::groups::app_data::load_component_registry;
    use tls_codec::{Serialize as _, VLBytes};
    use xmtp_configuration::{
        ApplicationComponentDefinition, ComponentPermissions, MetadataPolicy,
    };
    use xmtp_mls_group_state::inbox_id::InboxId;
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
    use xmtp_proto::xmtp::mls::message_contents::{
        ComponentType, metadata_policy::MetadataBasePolicy,
    };

    let map = ComponentId::new(0xC1A5);
    let policy = |base: MetadataBasePolicy| Some(MetadataPolicy::Base(base as i32));
    let alix = EngineTestPeer::new_with(|o| {
        o.configure(|c| {
            c.application_components = vec![ApplicationComponentDefinition {
                component_id: map.as_u16(),
                name: "app.owned".to_string(),
                component_type: ComponentType::TlsMapInboxIdBytes as i32,
                permissions: ComponentPermissions {
                    insert: policy(MetadataBasePolicy::Allow),
                    update: policy(MetadataBasePolicy::Allow),
                    delete: policy(MetadataBasePolicy::AllowIfSelfOrNonMember),
                },
                in_groups: true,
                in_dms: false,
            }]
        })
    })
    .await;
    let conversation = alix.create_group(None, None)?;
    let provider = alix.context.mls_provider();
    let signer = &alix.context.identity().installation_keys;
    let group = OpenMlsGroup::load(
        alix.context.mls_storage(),
        &conversation.group_id.to_openmls(),
    )??;
    let other = InboxId::from_bytes([7; 32]);
    let update = |id: ComponentId, delta: TlsMapDelta<InboxId, VLBytes>| {
        AppDataUpdateProposal::update(id.as_u16(), delta.tls_serialize_detached().unwrap())
    };
    let membership = ComponentId::GROUP_MEMBERSHIP;
    let join = update(
        membership,
        TlsMapDelta::new().insert(other, VLBytes::new(Vec::new())),
    );
    let leave = update(membership, TlsMapDelta::new().delete(other));
    let write = update(
        map,
        TlsMapDelta::new()
            .insert(other, VLBytes::new(b"v".to_vec()))
            .delete(other),
    );

    let staged = stage_listed_commit(
        &group,
        &provider,
        signer,
        &[join.clone(), leave, join, write],
    );
    assert_eq!(staged.queued_proposals().count(), 3);
    let (immutable, mutable) = read_committed_metadata(&group)?;
    let registry = load_component_registry(&group)?;
    let result = validate_app_data_update_proposals_in_commit(
        &staged, &group, &immutable, &mutable, &registry,
    );
    assert!(result.is_ok(), "{result:?}");
}
