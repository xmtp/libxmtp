use super::*;

#[rstest::rstest]
#[case::earlier(9, 10, true)]
#[case::same(10, 10, false)]
#[case::future(11, 10, false)]
#[xmtp_common::test(unwrap_try = true)]
async fn identity_references_precede_the_group_envelope(
    #[case] identity_sequence: u64,
    #[case] envelope_sequence: u64,
    #[case] valid: bool,
) {
    let mut membership = GroupMembership::new();
    membership.add("member".to_owned(), identity_sequence);
    let result = validate_identity_sequence_order(&membership, envelope_sequence);
    assert_eq!(result.is_ok(), valid);
    if let Err(error) = result {
        assert!(error.is_safe_rejection());
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn a_missing_identity_proof_cannot_advance_processing() {
    let requirement = IdentityRequirement {
        inbox_id: "member".to_owned(),
        sequence_id: 1,
    };
    let need = CommitValidationError::IdentityDependency(IdentityDependencyError::Need(
        requirement.clone(),
    ));
    assert!(!need.is_safe_rejection());
    let absent = CommitValidationError::IdentityDependency(
        IdentityDependencyError::MissingReference(requirement),
    );
    assert!(absent.is_safe_rejection());
    let unsupported = CommitValidationError::ProtocolVersionTooLow("999.0.0".to_owned());
    assert!(!unsupported.is_safe_rejection());
    let malformed = CommitValidationError::MissingMutableMetadata;
    assert!(malformed.is_safe_rejection());
    assert!(!CommitValidationError::installed_state(malformed).is_safe_rejection());
}
