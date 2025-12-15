use xmtp_proto::types::{Topic, TopicCursor};
use xmtp_proto::{api::VectorClock, types::GlobalCursor};

use crate::protocol::{ApplyCursor, Envelope, EnvelopeError, Sort};

pub struct CausalSort<'a, E> {
    envelopes: &'a mut Vec<E>,
    topic_cursor: &'a mut TopicCursor,
}

// store temporary info about the envelope
// so that we do not need to re-deserialize
struct Missed<E> {
    envelope: E,
    depends_on: GlobalCursor,
    topic: Topic,
}

impl<E> Missed<E> {
    pub fn new(envelope: E, depends_on: GlobalCursor, topic: Topic) -> Self {
        Self {
            envelope,
            depends_on,
            topic,
        }
    }

    pub fn into_envelope(self) -> E {
        self.envelope
    }
}

impl<'b, 'a: 'b, E: Envelope<'a>> CausalSort<'b, E> {
    // check if any of the dependencies of envelopes in `other` are
    // satisfied by any envelopes in `self.envelopes`
    // this lets us resolve dependencies internally
    // for deeply-nested sets of dependencies.
    fn recover_newly_valid(&mut self, missed: &mut Vec<Missed<E>>) -> Vec<E> {
        missed
            .extract_if(.., |m| {
                let clock = self.topic_cursor.get_or_default(&m.topic);
                clock.dominates(&m.depends_on)
            })
            .map(move |m| m.envelope)
            .collect()
    }
}

impl<'b, 'a: 'b, E: Envelope<'a>> Sort<Vec<E>> for CausalSort<'b, E> {
    fn sort(mut self) -> Result<Option<Vec<E>>, EnvelopeError> {
        let mut i = 0;
        let mut missed = Vec::new();
        while i < self.envelopes.len() {
            let env = &mut self.envelopes[i];
            let topic = env.topic()?;
            let last_seen = env.depends_on()?.unwrap_or(Default::default());
            let vector_clock = self.topic_cursor.get_or_default(&topic);
            if vector_clock.dominates(&last_seen) {
                self.topic_cursor.apply(env)?;
                let newly_valid = self.recover_newly_valid(&mut missed);
                i += 1;
                self.envelopes.splice(i..i, newly_valid.into_iter());
            } else {
                let missed_envelope = self.envelopes.remove(i);
                missed.push(Missed::new(missed_envelope, last_seen, topic));
            }
        }
        Ok((!missed.is_empty()).then_some(missed.into_iter().map(Missed::into_envelope).collect()))
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
                    .map(|e| e.cursor.to_string())
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
                    .map(|e| e.cursor.to_string())
                    .collect::<Vec<_>>(),
                missing
                    .iter()
                    .map(|e| e.cursor.to_string())
                    .collect::<Vec<_>>(),
                sorted
                    .iter()
                    .map(|e| e.cursor.to_string())
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

        #[xmtp_common::test]
        fn reapplies_within_array(mut envelopes in sorted_dependencies(10, vec![10, 20, 30, 40]).prop_shuffle()) {
            let mut topic_cursor = TopicCursor::default();
            let mut missing = vec![];
            if let Some(m) = sort::causal(&mut envelopes, &mut topic_cursor).sort()? {
                missing = m.to_vec();
            }
            assert_sorted(&envelopes, &missing, &[])
        }
    }
}
