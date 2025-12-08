use super::TestEnvelope;
use itertools::Itertools;
use proptest::prelude::*;
use proptest::sample::subsequence;
use xmtp_proto::{
    api::VectorClock,
    types::{Cursor, GlobalCursor, OriginatorId, SequenceId},
};

// Advance the clock for a given originator
fn advance_clock(base: &GlobalCursor, originator: &OriginatorId) -> SequenceId {
    base.get(originator) + 1
}

/// always creates a _sorted_ list of dependencies
pub fn sorted_dependencies(
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
                    prop::sample::subsequence(envelopes_clone.clone(), 0..=envelopes_len).prop_map(
                        move |envelopes_subset| {
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
                                cursor: Cursor::new(sequence_id, originator),
                                depends_on: new_clock,
                            });
                            envelopes
                        },
                    )
                })
            })
            .boxed()
    })
}

#[derive(Debug, Clone)]
pub struct EnvelopesWithMissing {
    pub removed: Vec<TestEnvelope>,
    pub envelopes: Vec<TestEnvelope>,
}

prop_compose! {
    pub fn missing_dependencies(length: usize, originators: Vec<OriginatorId>)(envelopes_o in sorted_dependencies(length, originators))(remove in subsequence(envelopes_o.clone(), 0..=envelopes_o.len()), mut envelopes in Just(envelopes_o)) -> EnvelopesWithMissing {
        envelopes.retain(|e| !remove.contains(e));
        EnvelopesWithMissing {
            removed: remove,
            envelopes,

        }
    }
}

/// check if `missing` depends on any envelope in `removed`
pub fn depends_on_one(missing: &TestEnvelope, removed: &[TestEnvelope]) -> bool {
    for envelope in removed {
        if missing.has_dependency_on(envelope) {
            return true;
        }
    }
    false
}
