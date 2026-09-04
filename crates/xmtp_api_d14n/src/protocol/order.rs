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
                let orphans = missing
                    .into_iter()
                    .map(|e| e.orphan())
                    .inspect(|orphan| {
                        if let Ok(o) = orphan {
                            tracing::debug!("icing {}", o)
                        }
                    })
                    .try_collect()?;
                self.store.ice(orphans)?;
                break;
            }
            self.envelopes.append(&mut resolved);
            self.envelopes.append(&mut missing);
            self.recover_lost_children()?;
        }
        Ok(())
    }

    fn order_offline(&mut self) -> Result<(), ResolutionError> {
        self.recover_lost_children()?;
        self.timestamp_sort()?;
        if let Some(missing) = self.causal_sort()? {
            tracing::debug!("icing {} orphans", missing.len());
            let orphans = missing
                .into_iter()
                .map(|e| e.orphan())
                .inspect(|orphan| {
                    if let Ok(o) = orphan {
                        tracing::debug!("icing {}", o)
                    }
                })
                .try_collect()?;
            self.store.ice(orphans)?;
        }
        Ok(())
    }
}
