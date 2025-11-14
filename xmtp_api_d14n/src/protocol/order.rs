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
                envelopes: resolved,
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
                // cannot use retain directly b/c curosr returns Result<>.
                // see https://github.com/xmtp/libxmtp/issues/2691
                // TODO:2691
                let mut to_remove = HashSet::new();
                for (i, m) in missing.iter().enumerate() {
                    if unresolved.contains(&m.cursor()?) {
                        to_remove.insert(i);
                    }
                }
                let mut i = 0;
                missing.retain(|_m| {
                    let could_not_resolve = to_remove.contains(&i);
                    i += 1;
                    !could_not_resolve
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

// TODO: tests
