//! Facts used by this client's internal subscribers.

use std::collections::HashSet;
use xmtp_db::{ConnectionError, DbQuery, group_message::StoredGroupMessage};
use xmtp_proto::types::GroupId;

use crate::worker::device_sync::preference_sync::PreferenceUpdate;
use xmtp_db::consent_record::{ConsentState as StoredConsentState, ConsentType};
use xmtp_events::{
    ClientEvent, ConsentChanged, ConsentEntityKind, ConsentState, DeletionCause, EventWriter,
    HmacKeysUpdated, MessageDeleted, MessageRef,
};

#[derive(Clone, Debug)]
pub enum InternalEvent {
    GroupJoined(GroupId),
    MessagesStored,
    PreferencesChanged {
        updates: Vec<PreferenceUpdate>,
        origin: PreferenceOrigin,
    },
    MessagesDeleted(Vec<StoredGroupMessage>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreferenceOrigin {
    Local,
    Sync,
}

pub(crate) fn emit_preference_updates(
    writer: &impl EventWriter<InternalEvent>,
    updates: Vec<PreferenceUpdate>,
    origin: PreferenceOrigin,
    db: &impl DbQuery,
) -> Result<(), ConnectionError> {
    emit_preference_updates_with_public(writer, updates.clone(), updates, origin, db)
}

pub(crate) fn emit_preference_updates_with_public(
    writer: &impl EventWriter<InternalEvent>,
    public_updates: Vec<PreferenceUpdate>,
    updates: Vec<PreferenceUpdate>,
    origin: PreferenceOrigin,
    db: &impl DbQuery,
) -> Result<(), ConnectionError> {
    if updates.is_empty() {
        return Ok(());
    }
    // Only the final value of each key is readable after the transaction.
    let mut seen_consents = HashSet::new();
    let mut seen_hmac = false;
    let mut public_updates: Vec<_> = public_updates
        .iter()
        .rev()
        .filter(|update| match update {
            PreferenceUpdate::Consent(record) => {
                seen_consents.insert((record.entity_type as i32, record.entity.clone()))
            }
            PreferenceUpdate::Hmac { .. } => !std::mem::replace(&mut seen_hmac, true),
        })
        .collect();
    public_updates.reverse();
    for update in public_updates {
        let client = match update {
            PreferenceUpdate::Consent(record) => {
                let is_sync_group = if record.entity_type == ConsentType::ConversationId {
                    hex::decode(&record.entity)
                        .ok()
                        .and_then(|raw| GroupId::try_from(raw.as_slice()).ok())
                        .map(|id| db.find_group(&id))
                        .transpose()?
                        .flatten()
                        .is_some_and(|group| group.conversation_type.is_virtual())
                } else {
                    false
                };
                if is_sync_group {
                    continue;
                }
                ClientEvent::ConsentChanged(ConsentChanged {
                    entity_kind: match record.entity_type {
                        ConsentType::ConversationId => ConsentEntityKind::Conversation,
                        ConsentType::InboxId => ConsentEntityKind::Inbox,
                    },
                    entity: record.entity.clone(),
                    state: match record.state {
                        StoredConsentState::Unknown => ConsentState::Unknown,
                        StoredConsentState::Allowed => ConsentState::Allowed,
                        StoredConsentState::Denied => ConsentState::Denied,
                    },
                })
            }
            PreferenceUpdate::Hmac { .. } => ClientEvent::HmacKeysUpdated(HmacKeysUpdated),
        };
        writer.emit(Some(client), None);
    }
    writer.emit(
        None,
        Some(InternalEvent::PreferencesChanged { updates, origin }),
    );
    Ok(())
}

pub(crate) fn emit_deleted_messages(
    writer: &impl EventWriter<InternalEvent>,
    messages: Vec<StoredGroupMessage>,
    cause: DeletionCause,
    db: &impl DbQuery,
) -> Result<(), ConnectionError> {
    if messages.is_empty() {
        return Ok(());
    }
    for message in &messages {
        if db
            .find_group(&message.group_id)?
            .is_some_and(|group| group.conversation_type.is_virtual())
        {
            continue;
        }
        let reference = MessageRef {
            group_id: message.group_id.to_vec(),
            message_id: message.id.clone(),
        };
        let event = ClientEvent::MessageDeleted(MessageDeleted {
            group_id: reference.group_id,
            message_id: reference.message_id,
            cause,
        });
        writer.emit(Some(event), None);
    }
    writer.emit(None, Some(InternalEvent::MessagesDeleted(messages)));
    Ok(())
}

pub(crate) fn emit_expired_messages(
    writer: &impl EventWriter<InternalEvent>,
    messages: Vec<StoredGroupMessage>,
    db: &impl DbQuery,
) -> Result<(), ConnectionError> {
    if messages.is_empty() {
        return Ok(());
    }
    for message in &messages {
        if db
            .find_group(&message.group_id)?
            .is_some_and(|group| group.conversation_type.is_virtual())
        {
            continue;
        }
        writer.emit(
            Some(ClientEvent::MessageExpired(MessageRef {
                group_id: message.group_id.to_vec(),
                message_id: message.id.clone(),
            })),
            None,
        );
    }
    writer.emit(None, Some(InternalEvent::MessagesDeleted(messages)));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{context::XmtpSharedContext, tester};
    use xmtp_db::{
        consent_record::StoredConsentRecord,
        group::{GroupMembershipState, StoredGroup},
        prelude::*,
    };
    use xmtp_events::{EventBus, EventFilter, EventKind};
    use xmtp_proto::types::ConversationType;

    // verifies: EVENT-003, EVENT-001
    #[xmtp_common::test(unwrap_try = true)]
    async fn sync_group_consent_reaches_workers_but_not_public_subscriptions() {
        tester!(alix, disable_workers);
        let db = alix.context.db();
        let group_id = GroupId::from([71; 16]);
        StoredGroup::builder()
            .id(group_id)
            .created_at_ns(xmtp_common::time::now_ns())
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id(alix.inbox_id().to_string())
            .conversation_type(ConversationType::Sync)
            .build()?
            .store(&db)?;

        let bus = EventBus::<InternalEvent>::new();
        let public = bus.subscribe(EventFilter::new([EventKind::ConsentChanged]), Some(10));
        let worker = bus.subscribe(
            EventFilter::default()
                .with_internal(|event| matches!(event, InternalEvent::PreferencesChanged { .. })),
            Some(10),
        );
        let record = StoredConsentRecord::new(
            ConsentType::ConversationId,
            StoredConsentState::Allowed,
            hex::encode(group_id),
        );
        emit_preference_updates(
            &bus,
            vec![PreferenceUpdate::Consent(record)],
            PreferenceOrigin::Local,
            &db,
        )?;
        assert!(public.drain().is_empty());
        assert_eq!(worker.drain().len(), 1);
    }

    // verifies: EVENT-010
    #[xmtp_common::test(unwrap_try = true)]
    async fn public_preference_events_use_final_values_without_changing_legacy_batch() {
        tester!(alix, disable_workers);
        let bus = EventBus::<InternalEvent>::new();
        let public = bus.subscribe(EventFilter::new([EventKind::ConsentChanged]), Some(10));
        let legacy = bus.subscribe(
            EventFilter::default()
                .with_internal(|event| matches!(event, InternalEvent::PreferencesChanged { .. })),
            Some(10),
        );
        let updates = vec![
            PreferenceUpdate::Consent(StoredConsentRecord::new(
                ConsentType::InboxId,
                StoredConsentState::Denied,
                "same-inbox".into(),
            )),
            PreferenceUpdate::Consent(StoredConsentRecord::new(
                ConsentType::InboxId,
                StoredConsentState::Allowed,
                "same-inbox".into(),
            )),
        ];
        emit_preference_updates(&bus, updates, PreferenceOrigin::Sync, &alix.context.db())?;
        assert!(matches!(
            public.drain().as_slice(),
            [xmtp_events::EventEnvelope {
                client: Some(ClientEvent::ConsentChanged(change)), ..
            }] if change.state == ConsentState::Allowed
        ));
        assert!(matches!(
            legacy.drain().as_slice(),
            [xmtp_events::EventEnvelope {
                internal: Some(InternalEvent::PreferencesChanged { updates, .. }), ..
            }] if updates.len() == 2
        ));
    }
}
