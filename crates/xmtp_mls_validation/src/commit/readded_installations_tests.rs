use std::collections::HashSet;

use super::extract_readded_installations;
use crate::ReaddAuthority;

// verifies: GMOD-052
#[xmtp_common::test(unwrap_try = true)]
fn test_extract_readded_installations_without_authority_returns_empty() {
    let authority = ReaddAuthority::None;
    let mut added = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);
    let mut removed = HashSet::from([vec![1, 2, 3], vec![7, 8, 9]]);
    let mut failed = HashSet::from([vec![4, 5, 6]]);

    let original_added = added.clone();
    let original_removed = removed.clone();
    let original_failed = failed.clone();

    let result = extract_readded_installations(authority, &mut added, &mut removed, &mut failed);

    // Should return empty set
    assert!(result.is_empty());
    // Should not modify any of the input sets
    assert_eq!(added, original_added);
    assert_eq!(removed, original_removed);
    assert_eq!(failed, original_failed);
}

// verifies: GMOD-052
#[xmtp_common::test(unwrap_try = true)]
fn test_extract_readded_installations_super_admin_both_types_of_readd() {
    let mut added = HashSet::from([vec![1], vec![2]]);
    let mut removed = HashSet::from([vec![1], vec![3], vec![4]]);
    let mut failed = HashSet::from([vec![1], vec![3], vec![5]]);

    let readded = extract_readded_installations(
        ReaddAuthority::SuperAdmin,
        &mut added,
        &mut removed,
        &mut failed,
    );

    assert_eq!(readded, HashSet::from([vec![1], vec![3]]));
    assert_eq!(added, HashSet::from([vec![2]]));
    assert_eq!(removed, HashSet::from([vec![4]]));
    // Matched Add+Remove is handled first, leaving its failed entry untouched.
    assert_eq!(failed, HashSet::from([vec![1], vec![5]]));
}

// verifies: GMOD-052
#[xmtp_common::test(unwrap_try = true)]
fn a_dm_member_readds_only_matched_pairs() {
    let mut added = HashSet::from([vec![1], vec![2]]);
    let mut removed = HashSet::from([vec![1], vec![3]]);
    let mut failed = HashSet::new();
    let readded = extract_readded_installations(
        ReaddAuthority::DmParticipant,
        &mut added,
        &mut removed,
        &mut failed,
    );
    assert_eq!(readded, HashSet::from([vec![1]]));
    assert_eq!(added, HashSet::from([vec![2]]));
    assert_eq!(removed, HashSet::from([vec![3]]));
}

// verifies: GMOD-052
#[xmtp_common::test(unwrap_try = true)]
fn a_dm_member_cannot_readd_a_failed_installation_alone() {
    let mut added = HashSet::new();
    let mut removed = HashSet::from([vec![1]]);
    let mut failed = removed.clone();
    let readded = extract_readded_installations(
        ReaddAuthority::DmParticipant,
        &mut added,
        &mut removed,
        &mut failed,
    );
    assert!(readded.is_empty());
    assert_eq!(removed, HashSet::from([vec![1]]));
    assert_eq!(failed, removed);
}
