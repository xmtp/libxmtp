use std::collections::HashSet;

use crate::protocol::{
    Envelope, EnvelopeError, OrderedEnvelopeCollection, ResolutionError, ResolveDependencies,
    Resolved, Sort, VectorClock, sort, types::MissingEnvelope,
};
use derive_builder::Builder;
use itertools::Itertools;
use xmtp_proto::types::TopicCursor;

/// Order dependencies of `Self` according to [XIP](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#335-cross-originator-message-ordering)
/// If dependencies are missing, this ordering will try to resolve them
/// and re-apply resolved dependencies to the front of the envelope list
/// construct this strategy with [`Ordered::builder`]
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "EnvelopeError"))]
pub struct Ordered<T, R> {
    envelopes: Vec<T>,
    resolver: R,
    topic_cursor: TopicCursor,
}

impl<T, R> Ordered<T, R> {
    pub fn into_parts(self) -> (Vec<T>, TopicCursor) {
        (self.envelopes, self.topic_cursor)
    }
}

impl<T: Clone, R: Clone> Ordered<T, R> {
    pub fn builder() -> OrderedBuilder<T, R> {
        OrderedBuilder::default()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T, R> OrderedEnvelopeCollection for Ordered<T, R>
where
    T: Envelope<'static>,
    R: ResolveDependencies<ResolvedEnvelope = T>,
{
    async fn order(&mut self) -> Result<(), ResolutionError> {
        let Self {
            envelopes,
            resolver,
            topic_cursor,
        } = self;
        sort::timestamp(envelopes).sort()?;
        while let Some(mut missing) = sort::causal(envelopes, topic_cursor).sort()? {
            let cursors = missing
                .iter()
                .map(|e| {
                    let dependencies = e.depends_on()?.unwrap_or(Default::default());
                    let topic = e.topic()?;
                    let topic_clock = topic_cursor.get_or_default(&topic);
                    let need = topic_clock.missing(&dependencies);
                    Ok(need
                        .into_iter()
                        .map(|c| MissingEnvelope::new(topic.clone(), c))
                        .collect::<HashSet<MissingEnvelope>>())
                })
                .flatten_ok()
                .collect::<Result<HashSet<MissingEnvelope>, EnvelopeError>>()?;
            let Resolved {
                resolved,
                unresolved,
            } = resolver.resolve(cursors).await?;
            if resolved.is_empty() {
                // if we cant resolve anything, break the loop
                break;
            }
            if let Some(unresolved) = unresolved {
                let unresolved = unresolved
                    .into_iter()
                    .map(|m| m.cursor)
                    .collect::<HashSet<_>>();
                // if the resolver fails to resolve some envelopes, ignore them.
                // delete unresolved envelopes from missing envelopes list.
                // cannot use retain directly b/c cursor returns Result<>.
                // see https://github.com/xmtp/libxmtp/issues/2691
                // TODO:2691
                let mut to_remove = HashSet::new();
                for (i, m) in missing.iter().enumerate() {
                    if unresolved.contains(&m.cursor()?) {
                        to_remove.insert(i);
                    }
                }
                let mut i = 0;
                // or, retain all resolved envelopes
                missing.retain(|_m| {
                    let resolved = to_remove.contains(&i);
                    i += 1;
                    !resolved
                });
            }
            // apply missing before resolved, so that the resolved
            // are applied to the topic cursor before the missing dependencies.
            // todo: maybe `VecDeque` better here?
            envelopes.splice(0..0, missing.into_iter());
            envelopes.splice(0..0, resolved.into_iter());
            sort::timestamp(envelopes).sort()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::protocol::utils::test::{EnvelopesWithMissing, TestEnvelope, missing_dependencies};
    use futures::FutureExt;
    use proptest::{prelude::*, sample::subsequence};
    use xmtp_proto::types::OriginatorId;

    // Simple mock resolver that holds available envelopes to resolve
    #[derive(Clone, Debug)]
    struct MockResolver {
        available: Vec<TestEnvelope>,
        unavailable: Vec<TestEnvelope>,
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl ResolveDependencies for MockResolver {
        type ResolvedEnvelope = TestEnvelope;

        async fn resolve(
            &self,
            missing: HashSet<MissingEnvelope>,
        ) -> Result<Resolved<TestEnvelope>, ResolutionError> {
            // Return envelopes that match the missing set
            let resolved = self
                .available
                .iter()
                .filter(|env| {
                    let cursor = env.cursor().unwrap();
                    let topic = env.topic().unwrap();
                    missing.contains(&MissingEnvelope::new(topic, cursor))
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
                let depends_on = envelope.depends_on().unwrap().unwrap_or_default();
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
                    panic!("topic clock should always be complete at conclusion of ordering. {} does not dominate envelope {} depending on {}", topic_clock, envelope.cursor, depends_on);
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
                let depends_on = available_env.depends_on().unwrap().unwrap_or_default();
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
