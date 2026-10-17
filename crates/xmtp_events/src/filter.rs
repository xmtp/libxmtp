use std::sync::Arc;

use crate::{ClientEvent, ConsentEntityKind, ContentTypeId, EventContext, EventKind};

type InternalPredicate<I> = dyn Fn(&I) -> bool + Send + Sync;

/// Selects public kinds and, for worker subscribers, internal facts.
pub struct EventFilter<I> {
    pub kinds: Vec<EventKind>,
    pub group_ids: Option<Vec<Vec<u8>>>,
    /// DM identifiers resolved from `group_ids` when the subscription starts.
    pub dm_identifiers: Vec<Vec<u8>>,
    pub content_types: Option<Vec<ContentTypeId>>,
    pub references_own_messages: bool,
    internal: Option<Arc<InternalPredicate<I>>>,
}

impl<I> Default for EventFilter<I> {
    fn default() -> Self {
        Self {
            kinds: Vec::new(),
            group_ids: None,
            dm_identifiers: Vec::new(),
            content_types: None,
            references_own_messages: false,
            internal: None,
        }
    }
}

impl<I> EventFilter<I> {
    pub fn new(kinds: impl IntoIterator<Item = EventKind>) -> Self {
        Self {
            kinds: kinds.into_iter().collect(),
            ..Self::default()
        }
    }

    /// The predicate must not call any event bus. Such calls fail immediately.
    pub fn with_internal(mut self, predicate: impl Fn(&I) -> bool + Send + Sync + 'static) -> Self {
        self.internal = Some(Arc::new(predicate));
        self
    }

    pub(crate) fn matches_internal(&self, value: &I) -> bool {
        self.internal
            .as_ref()
            .is_some_and(|predicate| predicate(value))
    }

    pub(crate) fn matches_public(&self, event: &ClientEvent, context: &EventContext) -> bool {
        if event.kind() == EventKind::Lagged {
            return true;
        }
        if !self.kinds.contains(&event.kind()) {
            return false;
        }

        if let Some(group_ids) = &self.group_ids {
            let named_group = if let ClientEvent::ConsentChanged(consent) = event
                && consent.entity_kind == ConsentEntityKind::Conversation
            {
                let Some(group_id) = decode_hex(&consent.entity) else {
                    return false;
                };
                Some(group_id)
            } else {
                event.group_id().map(|id| id.to_vec())
            };
            if let Some(group_id) = named_group
                && !group_ids.contains(&group_id)
                && !context
                    .dm_identifier
                    .as_ref()
                    .is_some_and(|dm| self.dm_identifiers.contains(dm))
            {
                return false;
            }
        }

        if let ClientEvent::MessageReceived(message) = event {
            if let Some(types) = &self.content_types
                && !message
                    .content_type
                    .as_ref()
                    .is_some_and(|id| types.contains(id))
            {
                return false;
            }
            if self.references_own_messages
                && (!context.references_own_messages
                    || !message.content_type.as_ref().is_some_and(|content_type| {
                        content_type.authority_id == "xmtp.org"
                            && matches!(
                                (content_type.type_id.as_str(), content_type.version_major),
                                ("reply", 1) | ("reaction", 2)
                            )
                    }))
            {
                return false;
            }
        }
        true
    }
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    bytes
        .chunks_exact(2)
        .map(|pair| {
            let hi = (pair[0] as char).to_digit(16)?;
            let lo = (pair[1] as char).to_digit(16)?;
            Some(((hi << 4) | lo) as u8)
        })
        .collect()
}
