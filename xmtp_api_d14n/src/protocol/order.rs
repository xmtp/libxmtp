use crate::protocol::{
    Envelope, EnvelopeError, OrderedEnvelopeCollection, ResolutionError, ResolveDependencies, Sort,
    VectorClock, sort, types::MissingEnvelope,
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
        while let Some(missing) = sort::causal(envelopes, topic_cursor).sort()? {
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
                        .collect::<Vec<MissingEnvelope>>())
                })
                .flatten_ok()
                .collect::<Result<Vec<MissingEnvelope>, EnvelopeError>>()?;
            let resolved = match resolver.resolve(cursors).await {
                // if resolution fails, drop the missing envelopes.
                // in this case, we will not process any of those envelopes
                // until the next query.
                Err(ResolutionError::ResolutionFailed) => {
                    return Ok(());
                }
                Err(e) => return Err(e),
                Ok(r) => r,
            };
            // apply missing before resolved, so that the resolved
            // are applied to the topic cursor before the missing dependencies.
            envelopes.splice(0..0, missing.into_iter());
            envelopes.splice(0..0, resolved.into_iter());
            sort::timestamp(envelopes).sort()?;
        }
        Ok(())
    }
}

// TODO: tests
