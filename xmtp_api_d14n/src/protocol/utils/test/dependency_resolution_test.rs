use std::collections::HashSet;

use crate::protocol::{ResolveDependencies, types::MissingEnvelope};
use xmtp_proto::types::{Cursor, Topic};

/// Test that all missing envelopes are found immediately on the first resolution attempt.
///
/// This function verifies that when all requested envelopes are available,
/// the resolver returns them all in a single resolution pass with no unresolved items.
///
/// # Type Parameters
/// * `R` - A resolver that implements `ResolveDependencies`
///
/// # Arguments
/// * `resolver` - The dependency resolver to test
/// * `missing` - Set of missing envelopes to resolve
/// * `expected_count` - Number of envelopes expected to be resolved
///
/// # Panics
/// * If resolution fails
/// * If the number of resolved envelopes doesn't match `expected_count`
/// * If there are any unresolved envelopes remaining
pub async fn test_resolve_all_found_immediately<R>(
    resolver: &R,
    missing: HashSet<MissingEnvelope>,
    expected_count: usize,
) where
    R: ResolveDependencies,
{
    let result = resolver.resolve(missing).await;

    assert!(result.is_ok(), "Resolution should succeed");
    let resolved = result.unwrap();
    assert_eq!(
        resolved.envelopes.len(),
        expected_count,
        "Should resolve exactly {} envelopes",
        expected_count
    );
    assert!(
        resolved.unresolved.is_none() || resolved.unresolved.as_ref().unwrap().is_empty(),
        "Should have no unresolved envelopes"
    );
}

/// Test that the resolver handles partial resolution correctly.
///
/// This function verifies that when only some of the requested envelopes are available,
/// the resolver returns what it can find and correctly reports the unresolved items.
///
/// # Type Parameters
/// * `R` - A resolver that implements `ResolveDependencies`
///
/// # Arguments
/// * `resolver` - The dependency resolver to test
/// * `missing` - Set of missing envelopes to resolve
/// * `expected_resolved_count` - Number of envelopes expected to be resolved
/// * `expected_unresolved` - Expected set of unresolved missing envelopes
///
/// # Panics
/// * If resolution fails
/// * If the number of resolved envelopes doesn't match `expected_resolved_count`
/// * If there are no unresolved envelopes when some are expected
/// * If the unresolved set doesn't match `expected_unresolved`
pub async fn test_resolve_partial_resolution<R>(
    resolver: &R,
    missing: HashSet<MissingEnvelope>,
    expected_resolved_count: usize,
    expected_unresolved: HashSet<MissingEnvelope>,
) where
    R: ResolveDependencies,
{
    let result = resolver.resolve(missing).await;

    assert!(result.is_ok(), "Resolution should succeed");
    let resolved = result.unwrap();

    assert_eq!(
        resolved.envelopes.len(),
        expected_resolved_count,
        "Should resolve exactly {} envelopes",
        expected_resolved_count
    );

    assert!(
        resolved.unresolved.is_some(),
        "Should have unresolved envelopes"
    );

    let unresolved = resolved.unresolved.unwrap();
    assert_eq!(
        unresolved.len(),
        expected_unresolved.len(),
        "Unresolved count should match"
    );

    for expected in &expected_unresolved {
        assert!(
            unresolved.contains(expected),
            "Should contain unresolved envelope {:?}",
            expected
        );
    }
}

/// Test that the resolver handles an empty missing set correctly.
///
/// This function verifies that when no envelopes need to be resolved,
/// the resolver returns an empty result without errors.
///
/// # Type Parameters
/// * `R` - A resolver that implements `ResolveDependencies`
///
/// # Arguments
/// * `resolver` - The dependency resolver to test
///
/// # Panics
/// * If resolution fails
/// * If any envelopes are returned
/// * If there are any unresolved envelopes
pub async fn test_resolve_empty_missing_set<R>(resolver: &R)
where
    R: ResolveDependencies,
{
    let missing = HashSet::new();
    let result = resolver.resolve(missing).await;

    assert!(result.is_ok(), "Resolution should succeed with empty set");
    let resolved = result.unwrap();
    assert!(resolved.envelopes.is_empty(), "Should resolve no envelopes");
    assert!(
        resolved.unresolved.is_none() || resolved.unresolved.as_ref().unwrap().is_empty(),
        "Should have no unresolved envelopes"
    );
}

/// Helper function to create a set of missing envelopes for testing.
///
/// # Arguments
/// * `topic` - The topic for the envelopes
/// * `cursors` - List of (originator_id, sequence_id) pairs
///
/// # Returns
/// A `HashSet` of `MissingEnvelope` instances
pub fn create_missing_set(topic: Topic, cursors: Vec<(u32, u64)>) -> HashSet<MissingEnvelope> {
    cursors
        .into_iter()
        .map(|(originator_id, sequence_id)| {
            MissingEnvelope::new(
                topic.clone(),
                Cursor {
                    originator_id,
                    sequence_id,
                },
            )
        })
        .collect()
}
