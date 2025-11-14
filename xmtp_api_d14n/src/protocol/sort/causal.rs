use xmtp_proto::types::TopicCursor;

use crate::protocol::{ApplyCursor, Envelope, EnvelopeError, Sort, VectorClock};

pub struct CausalSort<'a, E> {
    envelopes: &'a mut Vec<E>,
    topic_cursor: &'a mut TopicCursor,
}

impl<'b, 'a: 'b, E: Envelope<'a>> Sort<Vec<E>> for CausalSort<'b, E> {
    fn sort(self) -> Result<Option<Vec<E>>, EnvelopeError> {
        let mut i = 0;
        // cant use `Vec::extract_if` b/c we are returning results
        let mut missing = Vec::new();
        while i < self.envelopes.len() {
            let env = &mut self.envelopes[i];
            let topic = env.topic()?;
            let last_seen = env.depends_on()?.unwrap_or(Default::default());
            let vector_clock = self.topic_cursor.get_or_default(&topic);
            if vector_clock.dominates(&last_seen) {
                self.topic_cursor.apply(env)?;
                i += 1;
            } else {
                missing.push(self.envelopes.remove(i));
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
    use chrono::Utc;
    use itertools::Itertools;
    use proptest::prelude::*;
    use proptest::sample::subsequence;
    use std::sync::LazyLock;
    use xmtp_proto::types::{Cursor, GlobalCursor, OriginatorId, SequenceId, Topic, TopicKind};
    use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
    use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

    use super::*;

    static TOPIC: LazyLock<Topic> =
        LazyLock::new(|| Topic::new(TopicKind::GroupMessagesV1, vec![0, 1, 2]));

    #[derive(Clone, Debug, PartialEq)]
    struct TestEnvelope {
        cursor: Cursor,
        depends_on: GlobalCursor,
    }

    impl TestEnvelope {
        fn has_dependency_on(&self, other: &TestEnvelope) -> bool {
            let originator = other.cursor.originator_id;
            let depends_on_sid = self.depends_on.get(&originator);
            depends_on_sid == other.cursor.sequence_id
        }
    }

    impl std::fmt::Display for TestEnvelope {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "cursor {} depends on {}", &self.cursor, &self.depends_on)
        }
    }

    impl Envelope<'_> for TestEnvelope {
        fn topic(&self) -> Result<xmtp_proto::types::Topic, crate::protocol::EnvelopeError> {
            Ok(TOPIC.clone())
        }

        fn payload(&self) -> Result<Payload, crate::protocol::EnvelopeError> {
            unreachable!()
        }

        fn timestamp(&self) -> Option<chrono::DateTime<Utc>> {
            unreachable!()
        }

        fn client_envelope(&self) -> Result<ClientEnvelope, crate::protocol::EnvelopeError> {
            unreachable!()
        }

        fn group_message(
            &self,
        ) -> Result<Option<xmtp_proto::types::GroupMessage>, crate::protocol::EnvelopeError>
        {
            unreachable!()
        }

        fn welcome_message(
            &self,
        ) -> Result<Option<xmtp_proto::types::WelcomeMessage>, crate::protocol::EnvelopeError>
        {
            unreachable!()
        }

        fn consume<E>(&self, _extractor: E) -> Result<E::Output, crate::protocol::EnvelopeError>
        where
            Self: Sized,
            for<'a> crate::protocol::EnvelopeError:
                From<<E as crate::protocol::EnvelopeVisitor<'a>>::Error>,
            for<'a> E: crate::protocol::EnvelopeVisitor<'a> + crate::protocol::Extractor,
        {
            unreachable!()
        }

        fn cursor(&self) -> Result<xmtp_proto::types::Cursor, EnvelopeError> {
            Ok(self.cursor)
        }

        fn depends_on(&self) -> Result<Option<xmtp_proto::types::GlobalCursor>, EnvelopeError> {
            Ok(Some(self.depends_on.clone()))
        }
    }

    // Advance the clock for a given originator
    fn advance_clock(base: &GlobalCursor, originator: &OriginatorId) -> SequenceId {
        base.get(originator) + 1
    }

    /// always creates a _sorted_ list of dependencies
    fn sorted_dependencies(
        length: usize,
        originators: Vec<OriginatorId>,
    ) -> impl Strategy<Value = Vec<TestEnvelope>> {
        let init = Just(Vec::<TestEnvelope>::new()).boxed();

        (0..length).fold(init, move |acc_strategy, _| {
            let originators = originators.clone();

            acc_strategy
                .prop_flat_map(move |envelopes| {
                    let originators = originators.clone();
                    let envelopes_len = envelopes.len();

                    // Pick an originator for this envelope
                    prop::sample::select(originators).prop_flat_map(move |originator| {
                        let envelopes_clone = envelopes.clone();

                        // Choose 0 or more previous envelopes to depend on
                        prop::sample::subsequence(envelopes_clone.clone(), 0..=envelopes_len)
                            .prop_map(move |envelopes_subset| {
                                let mut envelopes = envelopes_clone.clone();
                                let total_clock: GlobalCursor = envelopes_clone
                                    .iter()
                                    .map(|e| (e.cursor.originator_id, e.cursor.sequence_id))
                                    .into_grouping_map()
                                    .max()
                                    .into();
                                let mut base = GlobalCursor::default();
                                envelopes_clone
                                    .iter()
                                    .filter(|e| e.cursor.originator_id == originator)
                                    .map(|e| e.cursor)
                                    .for_each(|c| base.apply(&c));

                                let new_clock = if envelopes_subset.is_empty() {
                                    // must inherit dependencies of earlier sequence ids from
                                    // same originator id
                                    base
                                } else {
                                    for cursor in envelopes_subset.iter().map(|e| &e.cursor) {
                                        base.apply(cursor);
                                    }
                                    base
                                };

                                // Advance clock for this originator
                                let sequence_id = advance_clock(&total_clock, &originator);

                                envelopes.push(TestEnvelope {
                                    cursor: Cursor {
                                        originator_id: originator,
                                        sequence_id,
                                    },
                                    depends_on: new_clock,
                                });
                                envelopes
                            })
                    })
                })
                .boxed()
        })
    }

    #[derive(Debug, Clone)]
    struct EnvelopesWithMissing {
        removed: Vec<TestEnvelope>,
        envelopes: Vec<TestEnvelope>,
    }
    // higher order composition
    // creates dependencies then randomly removes some
    prop_compose! {
        fn missing_dependencies(length: usize, originators: Vec<OriginatorId>)(envelopes_o in sorted_dependencies(length, originators))(remove in subsequence(envelopes_o.clone(), 0..=envelopes_o.len()), mut envelopes in Just(envelopes_o)) -> EnvelopesWithMissing {
            envelopes.retain(|e| !remove.contains(e));
            EnvelopesWithMissing {
                removed: remove,
                envelopes,

            }
        }
    }

    fn depends_on_one(missing: &TestEnvelope, removed: &[TestEnvelope]) -> bool {
        for envelope in removed {
            if missing.has_dependency_on(envelope) {
                return true;
            }
        }
        false
    }

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
