use std::collections::HashSet;

use crate::protocol::{
    CursorStore, Envelope, EnvelopeError, OrderedEnvelopeCollection, ResolutionError,
    ResolveDependencies, Resolved, Sort, sort, types::RequiredDependency,
};
use derive_builder::Builder;
use itertools::Itertools;
use tracing::Level;
use xmtp_proto::api::VectorClock;
use xmtp_proto::types::{Cursor, OrphanedEnvelope, TopicCursor};

/// Order dependencies of `Self` according to [XIP](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#335-cross-originator-message-ordering)
/// If dependencies are missing, this ordering will try to resolve them
/// and re-apply resolved dependencies to the front of the envelope list
/// construct this strategy with [`Ordered::builder`]
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "EnvelopeError"))]
pub struct Ordered<T, R, S> {
    envelopes: Vec<T>,
    resolver: R,
    topic_cursor: TopicCursor,
    store: S,
}

impl<T, R, S> Ordered<T, R, S>
where
    S: CursorStore,
    R: ResolveDependencies<ResolvedEnvelope = T>,
    T: Envelope<'static> + prost::Message + Default,
{
    /// get the missing dependencies in the form of a [`RequiredDependency`]
    fn required_dependencies(
        &mut self,
        missing: &[T],
    ) -> Result<HashSet<RequiredDependency>, EnvelopeError> {
        missing
            .iter()
            .map(|e| {
                let dependencies = e.depends_on()?.unwrap_or(Default::default());
                let topic = e.topic()?;
                let topic_clock = self.topic_cursor.get_or_default(&topic);
                let need = topic_clock.missing(&dependencies);
                let needed_by = e.cursor()?;
                Ok(need
                    .into_iter()
                    .map(move |c| RequiredDependency::new(topic.clone(), c, needed_by)))
            })
            .flatten_ok()
            .try_collect()
    }

    // convenient internal proxy to causal sorting
    fn causal_sort(&mut self) -> Result<Option<Vec<T>>, EnvelopeError> {
        sort::causal(&mut self.envelopes, &mut self.topic_cursor).sort()
    }

    // convenient internal proxy to timestamp sort
    fn timestamp_sort(&mut self) -> Result<(), EnvelopeError> {
        // timestamp sort never returns missing envelopes
        let _ = sort::timestamp(&mut self.envelopes).sort()?;
        Ok(())
    }

    /// try to find any lost children and re-apply them to the
    /// end of the envelopes list before any resolution occurs
    fn recover_lost_children(&mut self) -> Result<(), EnvelopeError> {
        let cursors: Vec<_> = self.envelopes.iter().map(|e| e.cursor()).try_collect()?;
        let children = self.store.resolve_children(&cursors)?;
        if !children.is_empty() {
            tracing::info!("recovered {} children", children.len());
            if tracing::enabled!(Level::TRACE) {
                for child in &children {
                    tracing::trace!(
                        "recovered child@{} dependant on parent@{} for group@{}",
                        &child.cursor,
                        &child.depends_on,
                        &child.group_id
                    );
                }
            }
        }
        let cursors: HashSet<Cursor> = HashSet::from_iter(cursors);
        let mut envelopes: Vec<T> = children
            .into_iter()
            // ensure we don't re-add duplicates from the db
            .filter(|o| !cursors.contains(&o.cursor))
            .map(OrphanedEnvelope::into_payload)
            .map(T::decode)
            .try_collect()?;
        // ensure we append them to the list so that the sorting
        // adds the parent envelopes to the topic cursor before the orphans
        self.envelopes.append(&mut envelopes);
        Ok(())
    }
}

impl<T, R, S> Ordered<T, R, S> {
    pub fn into_parts(self) -> (Vec<T>, TopicCursor) {
        (self.envelopes, self.topic_cursor)
    }
}

impl<T: Clone, R: Clone, S: Clone> Ordered<T, R, S> {
    pub fn builder() -> OrderedBuilder<T, R, S> {
        OrderedBuilder::default()
    }
}

#[xmtp_common::async_trait]
impl<T, R, S> OrderedEnvelopeCollection for Ordered<T, R, S>
where
    T: Envelope<'static> + prost::Message + Default,
    R: ResolveDependencies<ResolvedEnvelope = T>,
    S: CursorStore,
{
    // NOTE:
    // In the case where a child has multiple dependants, and one is still missing:
    // 1.) child is recovered
    // 2.) child is added to "missing"
    // 3.) resolution of missing is attempted
    // 4.) child re-iced if resolution failed
    async fn order(&mut self) -> Result<(), ResolutionError> {
        self.recover_lost_children()?;
        self.timestamp_sort()?;
        while let Some(mut missing) = self.causal_sort()? {
            let needed_envelopes = self.required_dependencies(&missing)?;
            let Resolved { mut resolved, .. } = self.resolver.resolve(needed_envelopes).await?;
            if resolved.is_empty() {
                let orphans = missing.into_iter().map(|e| e.orphan()).try_collect()?;
                self.store.ice(orphans)?;
                break;
            }
            self.envelopes.append(&mut resolved);
            self.envelopes.append(&mut missing);
            self.recover_lost_children()?;
            self.timestamp_sort()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::protocol::{
        NoCursorStore,
        utils::test::{EnvelopesWithMissing, TestEnvelope, missing_dependencies},
    };
    use futures::FutureExt;
    use proptest::{prelude::*, sample::subsequence};
    use xmtp_proto::types::OriginatorId;

    // Simple mock resolver that holds available envelopes to resolve
    #[derive(Clone, Debug)]
    struct MockResolver {
        available: Vec<TestEnvelope>,
        unavailable: Vec<TestEnvelope>,
    }

    #[xmtp_common::async_trait]
    impl ResolveDependencies for MockResolver {
        type ResolvedEnvelope = TestEnvelope;

        async fn resolve(
            &self,
            missing: HashSet<RequiredDependency>,
        ) -> Result<Resolved<TestEnvelope>, ResolutionError> {
            let missing_cursors = missing.iter().map(|m| m.cursor).collect::<HashSet<_>>();
            // Return envelopes that match the missing set
            let resolved = self
                .available
                .iter()
                .filter(|env| {
                    let cursor = env.cursor();
                    missing_cursors.contains(&cursor)
                })
                .cloned()
                .collect::<Vec<_>>();

            Ok(Resolved::new(resolved, None))
        }
    }

    prop_compose! {
        pub fn resolvable_dependencies(length: usize, originators: Vec<OriginatorId>)
            (envelopes in missing_dependencies(length, originators))
                (available in subsequence(envelopes.removed.clone(), envelopes.removed.len()), envelopes in Just(envelopes))
        -> EnvelopesWithResolver {
            let mut unavailable = envelopes.removed.clone();
            unavailable.retain(|e| !available.contains(e));
            EnvelopesWithResolver {
                missing: envelopes,
                resolver: MockResolver {
                    available,
                    unavailable
                }
            }
        }
    }

    #[derive(Debug, Clone)]
    struct EnvelopesWithResolver {
        missing: EnvelopesWithMissing,
        resolver: MockResolver,
    }
    proptest! {
        #[xmtp_common::test]
        fn orders_with_unresolvable_dependencies(
            envelopes in resolvable_dependencies(30, vec![10, 20, 30, 40, 50, 60])
        ) {
            let EnvelopesWithResolver {
                missing,
                resolver
            } = envelopes;

            let (available, unavailable) = (resolver.available.clone(), resolver.unavailable.clone());
            let mut ordered = Ordered::builder()
                .envelopes(missing.envelopes)
                .resolver(resolver)
                .store(NoCursorStore)
                .topic_cursor(TopicCursor::default())
                .build()
                .unwrap();

            // Perform ordering - some dependencies cannot be resolved
            ordered.order().now_or_never()
                .expect("Future should complete immediately")
                .unwrap();

            let (result, mut topic_cursor) = ordered.into_parts();

            // Check that no envelope in the result depends on an unavailable removed envelope
            for envelope in &result {
                let depends_on = envelope.depends_on();
                let topic = envelope.topic().unwrap();
                let topic_clock = topic_cursor.get_or_default(&topic);

                // If this envelope's dependencies are satisfied by the topic cursor,
                // it should not depend on any unavailable envelopes
                if topic_clock.dominates(&depends_on) {
                    for unavailable_env in &unavailable {
                        prop_assert!(
                            !envelope.has_dependency_on(unavailable_env),
                            "Envelope with satisfied dependencies should not depend on unavailable envelope. \
                             Envelope: {}, Unavailable: {}",
                            envelope,
                            unavailable_env
                        );
                    }
                } else {
                    panic!("topic clock should always be complete at conclusion of ordering. {} does not dominate envelope {} depending on {}", topic_clock, envelope.cursor(), depends_on);
                }
            }

            // Verify that envelopes which were made available are in the result
            // (unless they themselves depend on unavailable envelopes)
            for available_env in &available {
                //
                if available_env.has_dependency_on_any(&unavailable) { continue; }
                // none of the envelopes have a dependency on this one, so resolver wont care
                if result.iter().all(|e| !e.has_dependency_on(available_env)) { continue; }
                prop_assert!(
                    result.iter().any(|e| e == available_env),
                    "Result does not contain {}", available_env
                );
                // If it's in the result, verify its dependencies are satisfied
                let depends_on = available_env.depends_on();
                let topic = available_env.topic().unwrap();
                let topic_clock = topic_cursor.get_or_default(&topic);

                prop_assert!(
                    topic_clock.dominates(&depends_on),
                    "Available envelope in result should have satisfied dependencies. \
                     Envelope: {}, Topic clock: {}",
                    available_env,
                    topic_clock
                );
            }
        }
    }
}
