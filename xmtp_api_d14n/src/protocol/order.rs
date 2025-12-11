use std::collections::HashSet;

use crate::protocol::{
    CursorStore, Envelope, EnvelopeError, OrderedEnvelopeCollection, ResolutionError,
    ResolveDependencies, Resolved, Sort, sort, types::RequiredDependency,
};
use derive_builder::Builder;
use itertools::Itertools;
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
                tracing::trace!(
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
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use super::*;
    use crate::protocol::{
        InMemoryCursorStore,
        utils::test::{EnvelopesWithMissing, TestEnvelope, missing_dependencies},
    };
    use futures::FutureExt;
    use parking_lot::Mutex;
    use proptest::{prelude::*, sample::subsequence};
    use xmtp_common::{DebugDisplay, assert_ok};
    use xmtp_proto::types::OriginatorId;

    // Simple mock resolver that holds available envelopes to resolve
    #[derive(Clone, Debug)]
    struct MockResolver {
        available: Arc<Mutex<Vec<TestEnvelope>>>,
        unavailable: Arc<Mutex<Vec<TestEnvelope>>>,
        returned: Arc<Mutex<Vec<TestEnvelope>>>,
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
            let mut ret = self.returned.lock();
            ret.extend(resolved.clone());
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

    #[track_caller]
    fn assert_topic_cursor_seen(
        cursor: &TopicCursor,
        env: &TestEnvelope,
        message: &str,
    ) -> Result<(), TestCaseError> {
        let clock = cursor.get(&env.topic().unwrap()).unwrap();
        prop_assert!(clock.has_seen(&env.cursor()), "{}", message);
        Ok(())
    }

    #[track_caller]
    fn assert_dependencies_satisfied(
        env: &TestEnvelope,
        topic_cursor: &mut TopicCursor,
    ) -> Result<(), TestCaseError> {
        let topic = env.topic().unwrap();
        let clock = topic_cursor.get_or_default(&topic);
        prop_assert!(
            clock.dominates(&env.depends_on()),
            "Envelope {} should have satisfied dependencies. Topic clock: {}",
            env,
            clock
        );
        Ok(())
    }

    #[track_caller]
    fn assert_no_unavailable_deps(
        env: &TestEnvelope,
        unavailable: &[TestEnvelope],
    ) -> Result<(), TestCaseError> {
        for unavailable_env in unavailable {
            prop_assert!(
                !env.has_dependency_on(unavailable_env),
                "Envelope should not depend on unavailable envelope. Envelope: {}, Unavailable: {}",
                env,
                unavailable_env
            );
        }
        Ok(())
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
                    unavailable: Arc::new(Mutex::new(unavailable)),
                    returned: Arc::new(Mutex::new(Vec::new()))
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

        fn unavailable(&self) -> Vec<TestEnvelope> {
            let v = self.resolver.unavailable.lock();
            v.clone()
        }

        /// make a dependency at `idx` in `unavailable` available
        pub fn make_available(&self, idx: usize) -> TestEnvelope {
            let mut v = self.resolver.unavailable.lock();
            let mut available = self.resolver.available.lock();
            let new = v.remove(idx);
            available.push(new.clone());
            new
        }

        pub fn returned(&self) -> Vec<TestEnvelope> {
            let v = self.resolver.returned.lock();
            v.clone()
        }

        fn all_envelopes(&self) -> Vec<TestEnvelope> {
            self.available()
                .into_iter()
                .chain(self.missing.envelopes.clone())
                .chain(self.returned())
                .collect()
        }

        fn find_envelope(&self, cursor: &Cursor) -> Option<TestEnvelope> {
            self.all_envelopes()
                .into_iter()
                .find(|e| e.cursor() == *cursor)
        }

        fn has_unavailable_in_dependency_chain(&self, env: &TestEnvelope) -> bool {
            let unavailable = self.unavailable();

            // Check immediate dependencies
            if env.has_dependency_on_any(&unavailable) {
                return true;
            }

            // Check transitive dependencies
            let mut to_check = vec![env.depends_on()];
            while let Some(deps) = to_check.pop() {
                if deps.is_empty() {
                    continue;
                }

                for cursor in deps.cursors() {
                    if let Some(dep_env) = self.find_envelope(&cursor) {
                        if dep_env.has_dependency_on_any(&unavailable) {
                            return true;
                        }
                        if !dep_env.depends_on().is_empty() {
                            to_check.push(dep_env.depends_on());
                        }
                    }
                }
            }
            false
        }

        // get only dependencies that can be validly depended on
        // (none of the dependencies dependants are unavailable)
        pub fn only_valid_dependants(&self) -> Vec<TestEnvelope> {
            self.missing
                .envelopes
                .iter()
                .filter(|env| !self.has_unavailable_in_dependency_chain(env))
                .cloned()
                .collect()
        }
    }

    proptest! {
        #[xmtp_common::test]
        fn orders_with_unresolvable_dependencies(
            envelopes in resolvable_dependencies(10, vec![10, 20, 30])
        ) {
            let valid = envelopes.only_valid_dependants();
            let available = envelopes.available();
            let unavailable = envelopes.unavailable();
            let EnvelopesWithResolver {
                missing,
                resolver
            } = envelopes;
            let store = InMemoryCursorStore::new();
            let mut ordered = Ordered::builder()
                .envelopes(missing.envelopes)
                .resolver(resolver)
                .store(store.clone())
                .topic_cursor(TopicCursor::default())
                .build()
                .unwrap();

            // Perform ordering - some dependencies cannot be resolved
            ordered.order().now_or_never()
                .expect("Future should complete immediately")
                .unwrap();

            let (result, mut topic_cursor) = ordered.into_parts();

            // Verify all valid envelopes are seen by topic cursor
            for env in &valid {
                assert_topic_cursor_seen(
                    &topic_cursor,
                    env,
                    &format!("topic cursor {} must have seen {:?}\n\
                        ordering_pass: \n{}\n\
                        icebox: \n{}\n",
                        topic_cursor, env, result.format_enumerated(),store.icebox().format_enumerated()
                    ))?;
            }

            // Check that all envelopes in result have satisfied dependencies
            for envelope in &result {
                assert_dependencies_satisfied(envelope, &mut topic_cursor)?;
                // Envelopes with satisfied dependencies shouldn't depend on unavailable ones
                assert_no_unavailable_deps(envelope, &unavailable)?;
            }
            // Verify that envelopes which were made available are in the result
            // (unless they themselves depend on unavailable envelopes or aren't needed)
            for available_env in &available {
                if available_env.has_dependency_on_any(&unavailable) { continue; }
                if result.iter().all(|e| !e.has_dependency_on(available_env)) { continue; }

                prop_assert!(
                    result.iter().any(|e| e == available_env),
                    "Result does not contain {}", available_env
                );
                assert_dependencies_satisfied(available_env, &mut topic_cursor)?;
            }
        }

        #[xmtp_common::test]
        fn orders_with_recovered_children(
            envelopes in resolvable_dependencies(10, vec![10, 20, 30])
        ) {
            let valid = envelopes.only_valid_dependants();
            let valid_cursors = valid.iter().map(|e| e.cursor()).collect::<HashSet<_>>();
            let unavailable = envelopes.unavailable();
            let EnvelopesWithResolver {
                ref missing,
                ref resolver
            } = envelopes;

            let topic_cursor = TopicCursor::default();
            let envelopes_to_check = missing.envelopes.clone();
            let store = InMemoryCursorStore::new();
            let mut ordered = Ordered::builder()
                .envelopes(missing.envelopes.clone())
                .resolver(resolver.clone())
                .store(store.clone())
                .topic_cursor(topic_cursor)
                .build()
                .unwrap();

            // Perform ordering - some dependencies cannot be resolved, so children get iced
            ordered.order().now_or_never()
                .expect("Future should complete immediately")
                .unwrap();

            let (first_ordering_pass, topic_cursor) = ordered.into_parts();

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
                let newly_available = envelopes.make_available(0);

                // Create a new ordered instance with the newly available envelope
                let mut ordered = Ordered::builder()
                    .envelopes(vec![newly_available.clone()])
                    .resolver(resolver.clone())
                    .store(store.clone())
                    .topic_cursor(topic_cursor)
                    .build()
                    .unwrap();

                // Perform ordering again - this should recover children
                let orphan_count = store.orphan_count();

                ordered.order().now_or_never()
                    .expect("Future should complete immediately")
                    .unwrap();

                let (second_ordering_pass, mut new_topic_cursor) = ordered.into_parts();

                // If the newly available envelope had children, they should be recovered
                // (orphan count should decrease)
                let (had_children, returned) = {
                    let returned = store.resolve_children(&[newly_available.cursor()]);
                    assert_ok!(&returned);
                    let returned = returned.unwrap();
                    // Check if any orphan was a child of the newly available envelope
                    let had_children = !returned.is_empty();
                    (had_children, returned)
                };
                let had_children = orphan_count > 0 && had_children;
                let child_str = returned.format_list();
                let icebox_str = store.icebox().format_enumerated();
                let second_ordering_pass_str = second_ordering_pass.format_enumerated();
                let first_ordering_pass_str = first_ordering_pass.format_enumerated();
                let is_valid = valid_cursors.contains(&newly_available.cursor());
                let valid_children = returned.iter().map(TestEnvelope::from).filter(|c| c.only_depends_on(&valid)).collect::<Vec<_>>();
                let num_valid_children = valid_children.len();
                let valid_children_str = valid_children.format_enumerated();
                if had_children && is_valid {
                    prop_assert_eq!(1 + num_valid_children, second_ordering_pass.len(),
                        "valid orphans should be in envelopes list\n\
                        valid_children: \n{}\n\
                        icebox: \n{}\n\
                        final topic_cursor: \n{}\n\
                        first_ordering_pass: \n{}\n\
                        newly_available->{}\n\
                        second_ordering_pass:\n{}\n\
                        ", valid_children_str, icebox_str, new_topic_cursor, first_ordering_pass_str, newly_available, second_ordering_pass_str,
                    );
                    prop_assert!(
                        !second_ordering_pass.is_empty(),
                        "Expected children to be recovered when parent becomes available.\n \
                         Result length: {} \
                         \n newly_available: {} \
                         \nicebox:\n{} \
                         \nlen: {} \
                         \nrecovered_children:\n{}
                         \n topic cursor {:?}",
                        second_ordering_pass.len(),
                        newly_available,
                        icebox_str,
                        store.orphan_count(),
                        child_str,
                        new_topic_cursor
                    );
                }

                // Verify that all envelopes in the result have satisfied dependencies
                for envelope in &second_ordering_pass {
                    assert_dependencies_satisfied(envelope, &mut new_topic_cursor)?;
                }
            }
        }
    }
}
