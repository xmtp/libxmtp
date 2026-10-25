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
    let authority = ReaddAuthority::SuperAdmin;
    let mut added = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);
    let mut removed = HashSet::from([vec![1, 2, 3], vec![7, 8, 9], vec![10, 11, 12]]);
    let mut failed = HashSet::from([vec![7, 8, 9]]);

    let result = extract_readded_installations(authority, &mut added, &mut removed, &mut failed);

    // Should return both types: added+removed intersection AND failed+removed intersection
    assert_eq!(result, HashSet::from([vec![1, 2, 3], vec![7, 8, 9]]));
    // Should remove readded from added
    assert_eq!(added, HashSet::from([vec![4, 5, 6]]));
    // Should remove both types from removed
    assert_eq!(removed, HashSet::from([vec![10, 11, 12]]));
    // Should remove from failed
    assert!(failed.is_empty());
}

// verifies: GMOD-052
#[xmtp_common::test(unwrap_try = true)]
fn test_extract_readded_installations_super_admin_overlapping_scenarios() {
    // Test case where an installation appears in all three sets
    // This tests the order of operations (added+removed processed before failed+removed)
    let authority = ReaddAuthority::SuperAdmin;
    let mut added = HashSet::from([vec![1, 2, 3]]);
    let mut removed = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);
    let mut failed = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);

    let result = extract_readded_installations(authority, &mut added, &mut removed, &mut failed);

    // Installation [1,2,3] is in added+removed (counted first)
    // Installation [4,5,6] is in failed+removed (counted second)
    assert_eq!(result, HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]));
    // All sets should be empty
    assert!(added.is_empty());
    assert!(removed.is_empty());
    // 1,2,3 was already intersected against added+removed
    assert_eq!(failed, HashSet::from([vec![1, 2, 3]]));
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
