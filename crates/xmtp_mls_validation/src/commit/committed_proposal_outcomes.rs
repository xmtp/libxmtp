//! Outcomes of [`validate_committed_proposals`] for complete proposal lists.
//!
//! The function sees a commit's whole proposal list, inline and by
//! reference, so these tests build lists directly instead of staging
//! commits. `xmtp_mls` covers the same rule on staged commits.

use openmls::{
    messages::{
        proposals::{
            AppDataUpdateProposal, AppEphemeralProposal, CustomProposal, ExternalInitProposal,
            PreSharedKeyProposal, Proposal, ProposalType, ReInitProposal, RemoveProposal,
        },
        proposals_in::GroupContextExtensionProposalIn,
    },
    schedule::PreSharedKeyId,
};
use tls_codec::DeserializeBytes as _;
use xmtp_mls_common::app_data::component_id::ComponentId;

use super::{CommitRuleError, validate_committed_proposals};

const COMPONENT: u16 = 0x8001;
const OTHER_COMPONENT: u16 = 0x8002;

fn remove_member() -> Proposal {
    Proposal::Remove(Box::new(
        RemoveProposal::tls_deserialize_exact_bytes(&1u32.to_be_bytes()).unwrap(),
    ))
}

fn update(id: u16) -> Proposal {
    Proposal::AppDataUpdate(Box::new(AppDataUpdateProposal::update(id, b"v".to_vec())))
}

fn remove(id: u16) -> Proposal {
    Proposal::AppDataUpdate(Box::new(AppDataUpdateProposal::remove(id)))
}

fn psk() -> Proposal {
    Proposal::PreSharedKey(Box::new(PreSharedKeyProposal::new(
        PreSharedKeyId::external(b"psk".to_vec(), vec![0; 32]),
    )))
}

/// Every proposal kind outside the allowlist except `PreSharedKey`.
fn held_kinds() -> Vec<Proposal> {
    // No extensions.
    let gce = GroupContextExtensionProposalIn::tls_deserialize_exact_bytes(&[0]).unwrap();
    // group_id <1 byte>, version mls10, ciphersuite 0x0001, no extensions.
    let reinit = ReInitProposal::tls_deserialize_exact_bytes(&[1, 7, 0, 1, 0, 1, 0]).unwrap();
    vec![
        Proposal::GroupContextExtensions(Box::new(gce.into())),
        Proposal::ReInit(Box::new(reinit)),
        Proposal::ExternalInit(Box::new(ExternalInitProposal::from(vec![1]))),
        Proposal::SelfRemove,
        Proposal::AppEphemeral(Box::new(AppEphemeralProposal::new(COMPONENT, vec![1]))),
        Proposal::Custom(Box::new(CustomProposal::new(0xF000, vec![1]))),
    ]
}

fn check(proposals: &[Proposal]) -> Result<(), CommitRuleError> {
    validate_committed_proposals(proposals, |_| true)
}

fn assert_invalid_list(proposals: &[Proposal]) {
    let error = check(proposals).unwrap_err();
    assert!(
        matches!(error, CommitRuleError::InvalidAppDataUpdateList(id) if id == ComponentId::from(COMPONENT)),
        "expected an invalid list for {COMPONENT:#x}, got {error:?}"
    );
    assert!(error.is_safe_rejection(), "an invalid list is terminal");
}

/// verifies: GMOD-001
///
/// A commit carrying any unsupported kind other than `PreSharedKey` is held,
/// never rejected: a later client version may support it, so rejecting would
/// fork the group. The check covers the whole list, so a held kind after an
/// allowed one still holds the commit.
#[xmtp_common::test(unwrap_try = true)]
fn committed_proposal_outcomes_hold_unsupported_kinds() {
    for held in held_kinds() {
        let kind = held.proposal_type();
        let error = check(&[remove_member(), update(COMPONENT), held]).unwrap_err();
        assert!(
            matches!(error, CommitRuleError::UnsupportedProposalType(t) if t == kind),
            "{kind:?} must hold the commit, got {error:?}"
        );
        assert!(!error.is_safe_rejection(), "{kind:?} must not be terminal");
    }
}

/// verifies: GMOD-001
///
/// A committed `PreSharedKey` is a terminal rejection, but it never masks a
/// held kind in the same list: holding is the outcome that cannot fork.
#[xmtp_common::test(unwrap_try = true)]
fn committed_proposal_outcomes_reject_psk_unless_held() {
    let error = check(&[remove_member(), psk()]).unwrap_err();
    assert!(matches!(error, CommitRuleError::NoPSKSupport), "{error:?}");
    assert!(error.is_safe_rejection());

    let error = check(&[psk(), Proposal::SelfRemove]).unwrap_err();
    assert!(
        matches!(
            error,
            CommitRuleError::UnsupportedProposalType(ProposalType::SelfRemove)
        ),
        "a held kind outranks the PSK rejection, got {error:?}"
    );
}

/// verifies: GMOD-030
///
/// An Update and a Remove for one component conflict in either order, even
/// though the commit carries no `GroupContextExtensions` proposal.
#[xmtp_common::test(unwrap_try = true)]
fn committed_proposal_outcomes_reject_mixed_operations() {
    assert_invalid_list(&[update(COMPONENT), remove(COMPONENT)]);
    assert_invalid_list(&[remove(COMPONENT), update(COMPONENT)]);
}

/// verifies: GMOD-030
///
/// A component can be removed at most once per commit, even though the
/// commit carries no `GroupContextExtensions` proposal.
#[xmtp_common::test(unwrap_try = true)]
fn committed_proposal_outcomes_reject_repeated_remove() {
    assert_invalid_list(&[remove(COMPONENT), remove(COMPONENT)]);
}

/// verifies: GMOD-030
///
/// Removing a component that has no state in the pre-commit dictionary is an
/// invalid proposal under the same section.
#[xmtp_common::test(unwrap_try = true)]
fn committed_proposal_outcomes_reject_remove_without_state() {
    let error = validate_committed_proposals(&[remove(COMPONENT)], |id| {
        id != ComponentId::from(COMPONENT)
    })
    .unwrap_err();
    assert!(
        matches!(error, CommitRuleError::InvalidAppDataUpdateList(id) if id == ComponentId::from(COMPONENT)),
        "{error:?}"
    );
}

/// Valid lists stay valid: allowed membership kinds, successive Updates for
/// one component, and a single Remove of another component.
#[xmtp_common::test(unwrap_try = true)]
fn committed_proposal_outcomes_accept_valid_lists() {
    check(&[]).unwrap();
    check(&[
        remove_member(),
        update(COMPONENT),
        update(COMPONENT),
        remove(OTHER_COMPONENT),
    ])
    .unwrap();
}
