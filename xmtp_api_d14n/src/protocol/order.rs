use std::collections::HashSet;

use crate::protocol::{
    CursorStore, Envelope, EnvelopeError, OrderedEnvelopeCollection, ResolutionError,
    ResolveDependencies, Resolved, Sort, VectorClock, sort, types::RequiredDependency,
};
use derive_builder::Builder;
use itertools::Itertools;
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
                    .map(|c| RequiredDependency::new(topic.clone(), c, needed_by))
                    .collect::<HashSet<RequiredDependency>>())
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
            for child in &children {
                tracing::info!(
                    "recovered child@{} dependant on parent@{} for group@{}",
                    &child.cursor,
                    &child.depends_on,
                    &child.group_id
                );
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
        // self.timestamp_sort()?;
        while let Some(mut missing) = self.causal_sort()? {
            let needed_envelopes = self.required_dependencies(&missing)?;
            tracing::info!("topic_cursor: {:?}", self.topic_cursor);
            // tracing::info!("missing \n{}", needed_str);
            let Resolved {
                mut resolved,
                unresolved,
            } = self.resolver.resolve(needed_envelopes).await?;
            if resolved.is_empty() {
                let orphans = missing.into_iter().map(|e| e.orphan()).try_collect()?;
                self.store.ice(orphans)?;
                break;
            }
            tracing::info!("adding {:?}", &resolved);
            tracing::info!("adding {:?}", &missing);
            self.envelopes.append(&mut resolved);
            self.envelopes.append(&mut missing);
            // apply resolved before missing, so that the resolved
            // are applied to the topic cursor before the missing dependencies.
            // todo: maybe `VecDeque` better here?
            // self.envelopes.splice(0..0, missing.into_iter());
            // self.envelopes.splice(0..0, resolved.into_iter());
            self.recover_lost_children()?;
            // self.timestamp_sort()?;
        }
        // self.store.ice(orphans)?;
        //missing = new_missing.into_iter().map(|(_, m)| m).collect::<Vec<_>>();

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use super::*;
    use crate::protocol::{
        InMemoryCursorStore, NoCursorStore,
        utils::test::{EnvelopesWithMissing, TestEnvelope, missing_dependencies},
    };
    use futures::FutureExt;
    use parking_lot::Mutex;
    use proptest::{prelude::*, sample::subsequence};
    use xmtp_proto::types::OriginatorId;

    // Simple mock resolver that holds available envelopes to resolve
    #[derive(Clone, Debug)]
    struct MockResolver {
        available: Arc<Mutex<Vec<TestEnvelope>>>,
        unavailable: Vec<TestEnvelope>,
    }

    #[xmtp_common::async_trait]
    impl ResolveDependencies for MockResolver {
        type ResolvedEnvelope = TestEnvelope;

        async fn resolve(
            &self,
            missing: HashSet<RequiredDependency>,
        ) -> Result<Resolved<TestEnvelope>, ResolutionError> {
            let missing_set: HashSet<_> = missing.iter().map(|m| m.cursor).collect();
            let mut available = self.available.lock();
            let available_cursors = available
                .iter()
                .map(|a| a.cursor())
                .collect::<HashSet<Cursor>>();
            // Return envelopes that match the missing set
            let resolved = available
                .extract_if(.., |env| {
                    let cursor = env.cursor();
                    missing_set.contains(&cursor)
                })
                .collect::<Vec<_>>();
            let unresolved_cursors = missing_set
                .difference(&available_cursors)
                .collect::<HashSet<_>>();
            let unresolved: HashSet<_> = missing
                .into_iter()
                .filter(|m| unresolved_cursors.contains(&m.cursor))
                .collect();

            Ok(Resolved::new(
                resolved,
                (!unresolved.is_empty()).then_some(unresolved),
            ))
        }
    }

    prop_compose! {
        pub fn resolvable_dependencies(length: usize, originators: Vec<OriginatorId>)
            (envelopes in missing_dependencies(length, originators))
                (available in subsequence(envelopes.removed.clone(), 0..=envelopes.removed.len()), envelopes in Just(envelopes))
        -> EnvelopesWithResolver {
            let mut unavailable = envelopes.removed.clone();
            unavailable.retain(|e| !available.contains(e));
            EnvelopesWithResolver {
                missing: envelopes,
                resolver: MockResolver {
                    available: Arc::new(Mutex::new(available)),
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

    impl EnvelopesWithResolver {
        fn available(&self) -> Vec<TestEnvelope> {
            let a = self.resolver.available.lock();
            a.clone()
        }
        // get only dependencies that can be validly depended on
        // (none of the dependencies depednants are unavailable)
        pub fn only_valid_dependants(&self) -> Vec<TestEnvelope> {
            let mut valid = Vec::new();
            let to_be_ordered = &self.missing.envelopes;
            let unavailable = &self.resolver.unavailable;
            let available = self.available();
            // ensure topic clock properly reflects all envelopes
            for env in to_be_ordered {
                // ensure that the envelope does not depend on an unavilable
                if env.has_dependency_on_any(&unavailable) {
                    continue;
                }
                let mut abort = false;
                let mut depends = vec![env.depends_on()];
                let mut new_depends = vec![];
                // we not only have to check that this dependencies deps aren't unavailable, but
                // every dependency up the chain
                while !depends.is_empty() && !abort {
                    for dep in &depends {
                        if dep.cursors().any(|d| {
                            let e = available
                                .iter()
                                .chain(to_be_ordered)
                                .find(|e| d == e.cursor())
                                .unwrap();
                            if !e.has_dependency_on_any(&unavailable) && !e.depends_on().is_empty()
                            {
                                new_depends.push(e.depends_on());
                                false
                            } else {
                                true
                            }
                        }) {
                            abort = true;
                            break;
                        }
                        if abort {
                            break;
                        }
                    }
                    std::mem::swap(&mut depends, &mut new_depends);
                    new_depends.clear();
                }
                if abort {
                    continue;
                }
                valid.push(env.clone());
            }
            valid
        }
    }

    proptest! {
        #[xmtp_common::test]
        fn orders_with_unresolvable_dependencies(
            envelopes in resolvable_dependencies(10, vec![10, 20, 30])
        ) {
            let valid = envelopes.only_valid_dependants();
            let available = envelopes.available();
            let EnvelopesWithResolver {
                missing,
                resolver
            } = envelopes;
            let unavailable = resolver.unavailable.clone();
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

            for env in valid {
                let clock = topic_cursor.get_or_default(&env.topic().unwrap());
                prop_assert!(clock.has_seen(&env.cursor()), "topic cursor {:?} must have seen {}", topic_cursor, env.cursor());
            }

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

        #[xmtp_common::test]
        fn orders_with_recovered_children(
            envelopes in resolvable_dependencies(10, vec![10, 20, 30])
        ) {
            let mut valid = envelopes.only_valid_dependants();
            let mut valid_cursors = valid.iter().map(|e| e.cursor()).collect::<HashSet<_>>();
            let EnvelopesWithResolver {
                missing,
                mut resolver
            } = envelopes;

            let topic_cursor = TopicCursor::default();
            let unavailable = resolver.unavailable.clone();
            let envelopes_to_check = missing.envelopes.clone();
            let store = InMemoryCursorStore::new();
            let mut ordered = Ordered::builder()
                .envelopes(missing.envelopes)
                .resolver(resolver.clone())
                .store(store.clone())
                .topic_cursor(topic_cursor)
                .build()
                .unwrap();

            // Perform ordering - some dependencies cannot be resolved, so children get iced
            ordered.order().now_or_never()
                .expect("Future should complete immediately")
                .unwrap();

            let (_, topic_cursor) = ordered.into_parts();
            tracing::info!("TOPIC CURSOR {:?}", &topic_cursor);

            // Verify that the store has some orphaned envelopes
            let orphan_count = store.orphan_count();

            // If there were unavailable dependencies and envelopes depending on them,
            // we should have some orphans
            if !unavailable.is_empty() {
                let has_dependent_envelopes = envelopes_to_check.iter().any(|e| {
                    unavailable.iter().any(|u| e.has_dependency_on(u))
                });

                if has_dependent_envelopes {
                    prop_assert!(
                        orphan_count > 0,
                        "Expected some envelopes to be iced when dependencies are unavailable"
                    );
                }
            }

            // Now simulate a scenario where one of the unavailable envelopes becomes available
            // and we do another ordering pass - the children should be recovered
            if !unavailable.is_empty() && orphan_count > 0 {
                // make the first unavailable envelope available again
                let newly_available = {
                    let mut available = resolver.available.lock();
                    let newly_available = resolver.unavailable.remove(0);
                    available.push(newly_available.clone());
                    if newly_available.only_depends_on(&valid) {
                        valid.push(newly_available.clone());
                        valid_cursors.insert(newly_available.cursor());
                    }
                    newly_available
                };

                // Create a new ordered instance with the newly available envelope
                let mut ordered = Ordered::builder()
                    .envelopes(vec![newly_available.clone()])
                    .resolver(resolver)
                    .store(store.clone())
                    .topic_cursor(topic_cursor)
                    .build()
                    .unwrap();

                // Perform ordering again - this should recover children
                let orphan_count = store.orphan_count();

                ordered.order().now_or_never()
                    .expect("Future should complete immediately")
                    .unwrap();

                let (new_result, mut new_topic_cursor) = ordered.into_parts();

                // If the newly available envelope had children, they should be recovered
                // (orphan count should decrease)
                let (had_children, returned) = {
                    let returned = store.resolve_children(&[newly_available.cursor()]).unwrap();
                    // Check if any orphan was a child of the newly available envelope
                    let had_children = returned.len() > 0;
                    (had_children, returned)
                };
                let had_children = orphan_count > 0 && had_children;
                let child_str = returned.iter().fold(String::new(), |mut s, r| {
                    s.push_str(&format!("{:?}", r));
                    s.push('\n');
                    s
                });
                let icebox_str = store.icebox().iter().enumerate().fold(String::new(), |mut s, (i, r)| {
                    s.push_str(&format!("{} -- {:?}", i, r));
                    s.push('\n');
                    s
                });
                let is_valid = valid_cursors.contains(&newly_available.cursor());
                let num_valid_children = returned.iter().map(TestEnvelope::from).filter(|c| c.only_depends_on(&valid)).count();

                if had_children && is_valid {
                    prop_assert_eq!(1 + num_valid_children, new_result.len(), "valid orphans should be in envelopes list, result {:?}", new_result);
                    prop_assert!(
                        new_result.len() >= 1,
                        "Expected children to be recovered when parent becomes available.\n \
                         Result length: {} \
                         \n newly_available: {} \
                         \nicebox:\n{} \
                         \nlen: {} \
                         \nrecovered_children:\n{}
                         \n topic cursor {:?}",
                        new_result.len(),
                        newly_available,
                        icebox_str,
                        store.orphan_count(),
                        child_str,
                        new_topic_cursor
                    );
                }

                // Verify that all envelopes in the result have satisfied dependencies
                for envelope in &new_result {
                    let depends_on = envelope.depends_on();
                    let topic = envelope.topic().unwrap();
                    let topic_clock = new_topic_cursor.get_or_default(&topic);

                    prop_assert!(
                        topic_clock.dominates(&depends_on),
                        "Recovered envelope should have satisfied dependencies. \
                         Envelope: {}, Topic clock: {}",
                        envelope,
                        topic_clock
                    );
                }
            }
        }
    }
}
