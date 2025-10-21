use std::collections::HashSet;

use crate::groups::validated_commit::{CommitParticipant, extract_readded_installations};

fn create_test_actor(is_super_admin: bool) -> CommitParticipant {
    CommitParticipant {
        inbox_id: "test_inbox".to_string(),
        installation_id: vec![1, 2, 3],
        is_creator: false,
        is_admin: false,
        is_super_admin,
    }
}

#[test]
fn test_extract_readded_installations_non_super_admin_returns_empty() {
    let actor = create_test_actor(false);
    let mut added = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);
    let mut removed = HashSet::from([vec![1, 2, 3], vec![7, 8, 9]]);
    let mut failed = HashSet::from([vec![4, 5, 6]]);

    let original_added = added.clone();
    let original_removed = removed.clone();
    let original_failed = failed.clone();

    let result = extract_readded_installations(&actor, &mut added, &mut removed, &mut failed);

    // Should return empty set
    assert!(result.is_empty());
    // Should not modify any of the input sets
    assert_eq!(added, original_added);
    assert_eq!(removed, original_removed);
    assert_eq!(failed, original_failed);
}

#[test]
fn test_extract_readded_installations_super_admin_added_and_removed_intersection() {
    let actor = create_test_actor(true);
    let mut added = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);
    let mut removed = HashSet::from([vec![1, 2, 3], vec![7, 8, 9]]);
    let mut failed = HashSet::new();

    let result = extract_readded_installations(&actor, &mut added, &mut removed, &mut failed);

    // Should return the intersection
    assert_eq!(result, HashSet::from([vec![1, 2, 3]]));
    // Should remove from both added and removed
    assert_eq!(added, HashSet::from([vec![4, 5, 6]]));
    assert_eq!(removed, HashSet::from([vec![7, 8, 9]]));
    // Failed should remain unchanged
    assert!(failed.is_empty());
}

#[test]
fn test_extract_readded_installations_super_admin_failed_and_removed_intersection() {
    let actor = create_test_actor(true);
    let mut added = HashSet::from([vec![1, 2, 3]]);
    let mut removed = HashSet::from([vec![4, 5, 6], vec![7, 8, 9]]);
    let mut failed = HashSet::from([vec![4, 5, 6]]);

    let result = extract_readded_installations(&actor, &mut added, &mut removed, &mut failed);

    // Should return the failed/removed intersection
    assert_eq!(result, HashSet::from([vec![4, 5, 6]]));
    // Added should remain unchanged
    assert_eq!(added, HashSet::from([vec![1, 2, 3]]));
    // Should remove from both removed and failed
    assert_eq!(removed, HashSet::from([vec![7, 8, 9]]));
    assert!(failed.is_empty());
}

#[test]
fn test_extract_readded_installations_super_admin_both_types_of_readd() {
    let actor = create_test_actor(true);
    let mut added = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);
    let mut removed = HashSet::from([vec![1, 2, 3], vec![7, 8, 9], vec![10, 11, 12]]);
    let mut failed = HashSet::from([vec![7, 8, 9]]);

    let result = extract_readded_installations(&actor, &mut added, &mut removed, &mut failed);

    // Should return both types: added+removed intersection AND failed+removed intersection
    assert_eq!(result, HashSet::from([vec![1, 2, 3], vec![7, 8, 9]]));
    // Should remove readded from added
    assert_eq!(added, HashSet::from([vec![4, 5, 6]]));
    // Should remove both types from removed
    assert_eq!(removed, HashSet::from([vec![10, 11, 12]]));
    // Should remove from failed
    assert!(failed.is_empty());
}

#[test]
fn test_extract_readded_installations_super_admin_no_intersections() {
    let actor = create_test_actor(true);
    let mut added = HashSet::from([vec![1, 2, 3]]);
    let mut removed = HashSet::from([vec![4, 5, 6]]);
    let mut failed = HashSet::from([vec![7, 8, 9]]);

    let result = extract_readded_installations(&actor, &mut added, &mut removed, &mut failed);

    // Should return empty set when no intersections
    assert!(result.is_empty());
    // All sets should remain unchanged
    assert_eq!(added, HashSet::from([vec![1, 2, 3]]));
    assert_eq!(removed, HashSet::from([vec![4, 5, 6]]));
    assert_eq!(failed, HashSet::from([vec![7, 8, 9]]));
}

#[test]
fn test_extract_readded_installations_super_admin_empty_sets() {
    let actor = create_test_actor(true);
    let mut added = HashSet::new();
    let mut removed = HashSet::new();
    let mut failed = HashSet::new();

    let result = extract_readded_installations(&actor, &mut added, &mut removed, &mut failed);

    // Should return empty set
    assert!(result.is_empty());
    // All sets should remain empty
    assert!(added.is_empty());
    assert!(removed.is_empty());
    assert!(failed.is_empty());
}

#[test]
fn test_extract_readded_installations_super_admin_all_installations_readded() {
    let actor = create_test_actor(true);
    let mut added = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);
    let mut removed = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);
    let mut failed = HashSet::new();

    let result = extract_readded_installations(&actor, &mut added, &mut removed, &mut failed);

    // Should return all installations
    assert_eq!(result, HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]));
    // Both added and removed should be empty
    assert!(added.is_empty());
    assert!(removed.is_empty());
    assert!(failed.is_empty());
}

#[test]
fn test_extract_readded_installations_super_admin_multiple_failed_intersections() {
    let actor = create_test_actor(true);
    let mut added = HashSet::new();
    let mut removed = HashSet::from([vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]]);
    let mut failed = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);

    let result = extract_readded_installations(&actor, &mut added, &mut removed, &mut failed);

    // Should return both failed installations
    assert_eq!(result, HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]));
    // Added remains empty
    assert!(added.is_empty());
    // Removed should only have non-failed installation
    assert_eq!(removed, HashSet::from([vec![7, 8, 9]]));
    // Failed should be empty
    assert!(failed.is_empty());
}

#[test]
fn test_extract_readded_installations_super_admin_overlapping_scenarios() {
    // Test case where an installation appears in all three sets
    // This tests the order of operations (added+removed processed before failed+removed)
    let actor = create_test_actor(true);
    let mut added = HashSet::from([vec![1, 2, 3]]);
    let mut removed = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);
    let mut failed = HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]);

    let result = extract_readded_installations(&actor, &mut added, &mut removed, &mut failed);

    // Installation [1,2,3] is in added+removed (counted first)
    // Installation [4,5,6] is in failed+removed (counted second)
    assert_eq!(result, HashSet::from([vec![1, 2, 3], vec![4, 5, 6]]));
    // All sets should be empty
    assert!(added.is_empty());
    assert!(removed.is_empty());
    // 1,2,3 was already intersected against added+removed
    assert_eq!(failed, HashSet::from([vec![1, 2, 3]]));
}
