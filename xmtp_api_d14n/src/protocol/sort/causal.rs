use xmtp_proto::api::VectorClock;
use xmtp_proto::types::TopicCursor;

use crate::protocol::{ApplyCursor, Envelope, EnvelopeError, Sort};

pub struct CausalSort<'a, E> {
    envelopes: &'a mut Vec<E>,
    topic_cursor: &'a mut TopicCursor,
}

impl<'a, E: Envelope<'a>> CausalSort<'a, E> {
    // check if any of the dependencies of envelopes in `other` are
    // satisfied by envelopes in `self.envelopes`.
    // this lets us resolve dependencies internally
    // for deeply-nested sets of dependencies.
    fn valid_exist(&self, other: &[E]) -> bool {
        todo!()
    }
}

impl<'b, 'a: 'b, E: Envelope<'a>> Sort<Vec<E>> for CausalSort<'b, E> {
    fn sort(self) -> Result<Option<Vec<E>>, EnvelopeError> {
        let mut i = 0;
        // cant use `Vec::extract_if` b/c we are returning results
        let mut missing = Vec::new();
        // keep track of missing envelopes we've seen, so we can potentially re-apply
        // an envelope, if a past envelope in the array depends on a future envelope.
        let mut missing_cursor = self.topic_cursor.clone();
        while i < self.envelopes.len() {
            let env = &mut self.envelopes[i];
            let topic = env.topic()?;
            let last_seen = env.depends_on()?.unwrap_or(Default::default());
            let vector_clock = self.topic_cursor.get_or_default(&topic);
            if vector_clock.dominates(&last_seen) {
                self.topic_cursor.apply(env)?;
                missing_cursor.apply(env)?;
                i += 1;
            } else {
                let missing_envelope = self.envelopes.remove(i);
                missing.push(missing_envelope);
            }
        }
        Ok((!missing.is_empty()).then_some(missing))
    }
}

/// Sorts Envelopes Causally in-place
/// All envelopes part of `Self` will be sorted. Envelopes with missing
/// dependencies will be returned. `TopicCursor` will be updated to reflect
/// causally-verified envelopes.
/// [XIP Definition](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#causal-ordering)
/// A causal sort orders envelopes by their dependencies.
/// A `dependency` is defined as an envelope which must be processed before any
/// dependant envelopes.
/// ```text
/// Unsorted (arrival order):        Sorted (causal order):
///
///   [E3] ──depends on──> [E1]         [E1] ───→ [E2] ───→ [E3]
///   [E1] (no deps)                     ↑         ↑
///   [E2] ──depends on──> [E1]          │         │
///                                   (no deps) (needs E1)
///
/// E1 must be processed first, then E2, then E3
/// ```
/// # Arguments
/// * `envelopes`: the [`Envelope`]'s being sorted
/// * `topic_cursor`: the cursor position of all known topics
///
pub fn causal<'b, 'a: 'b, E: Envelope<'a>>(
    envelopes: &'b mut Vec<E>,
    topic_cursor: &'b mut TopicCursor,
) -> impl Sort<Vec<E>> + use<'a, 'b, E> {
    CausalSort {
        envelopes,
        topic_cursor,
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::sort;
    use crate::protocol::utils::test::{
        EnvelopesWithMissing, TestEnvelope, depends_on_one, missing_dependencies,
        sorted_dependencies,
    };
    use proptest::prelude::*;

    use super::*;
    fn assert_sorted(sorted: &[TestEnvelope], missing: &[TestEnvelope], removed: &[TestEnvelope]) {
        let mut missing_and_removed = removed.to_vec();
        missing_and_removed.extend(missing.to_vec().iter().cloned());
        for envelope in missing {
            assert!(
                // verify the missing envelope has a dependency on a removed or a different missing
                // envelope
                depends_on_one(envelope, missing_and_removed.as_slice()),
                "{envelope} has no dependency that is missing. missing & removed: {:?}",
                missing_and_removed
                    .iter()
                    .map(|e| e.cursor().to_string())
                    .collect::<Vec<_>>()
            );
        }
        // ensure the ones that are sorted do not depend on any that are removed
        for envelope in sorted {
            assert!(
                !depends_on_one(envelope, missing_and_removed.as_slice()),
                "{envelope} depends on a missing or removed dependency in \nremoved: {:?}, \nmissing: {:?} but it was marked as sorted,\n sorted {:?}",
                removed
                    .iter()
                    .map(|e| e.cursor().to_string())
                    .collect::<Vec<_>>(),
                missing
                    .iter()
                    .map(|e| e.cursor().to_string())
                    .collect::<Vec<_>>(),
                sorted
                    .iter()
                    .map(|e| e.cursor().to_string())
                    .collect::<Vec<_>>(),
            );
        }
    }

    proptest! {
        #[xmtp_common::test]
        fn causal_sort(envelopes in missing_dependencies(10, vec![10, 20, 30, 40])) {
            let mut topic_cursor = TopicCursor::default();
            let EnvelopesWithMissing { mut envelopes, removed, .. } = envelopes;
            let mut missing = vec![];
            if let Some(m) = sort::causal(&mut envelopes, &mut topic_cursor).sort()? {
                missing = m.to_vec();
            }
            assert_sorted(&envelopes, &missing, &removed)
        }

        /// this sort does not handle dependencies that are already available _within_ the given
        /// dependency array.
        #[xmtp_common::test]
        #[should_panic]
        fn does_not_reapply_within_array(mut envelopes in sorted_dependencies(10, vec![10, 20, 30, 40]).prop_shuffle()) {

            let mut topic_cursor = TopicCursor::default();
            let mut missing = vec![];
            if let Some(m) = sort::causal(&mut envelopes, &mut topic_cursor).sort()? {
                missing = m.to_vec();
            }
            assert_sorted(&envelopes, &missing, &[])
        }
    }
}
