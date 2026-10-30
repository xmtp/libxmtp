//! Facts used by this client's internal subscribers.

use xmtp_db::{
    ConnectionError, DbQuery,
    consent_record::{
        ConsentChange, ConsentKey, ConsentState as StoredConsentState, ConsentType, ConsentValue,
        StoredConsentRecord,
    },
    group_message::StoredGroupMessage,
    user_preferences::StoredUserPreferences,
};
use xmtp_mls_common::group_metadata::DmMembers;
use xmtp_proto::types::GroupId;

use crate::worker::device_sync::preference_sync::PreferenceUpdate;
use xmtp_events::{
    ClientEvent, ConsentChanged, ConsentEntityKind, ConsentState, DeletionCause, EventContext,
    EventWriter, HmacKeysUpdated, MessageDeleted, MessageRef,
};

#[derive(Clone, Debug)]
pub enum InternalEvent {
    GroupJoined {
        group_id: GroupId,
        is_sync: bool,
    },
    MessageStored {
        group_id: GroupId,
        message_id: Vec<u8>,
        expires_at_ns: Option<i64>,
        is_sync: bool,
    },
    /// A consent record or the root key was marked owed.
    OutboxChanged,
    TaskScheduled,
    NotificationSettingsChanged,
    /// Stored consent and root-key changes, for the legacy in-process streams.
    PreferencesChanged {
        updates: Vec<PreferenceUpdate>,
    },
    /// Deleted rows whose expiry had not passed when the event was emitted.
    /// A row can expire before delivery, so a reader checks it again.
    MessagesDeleted(DeletedMessages),
    /// Deleted rows whose expiry had passed: expiry cleanup, or a delete of
    /// an already expired message. Their bodies are cleared.
    MessagesExpired(DeletedMessages),
}

/// Rows removed by one delete, and the inbox that deleted them.
#[derive(Clone, Debug)]
pub struct DeletedMessages {
    pub messages: Vec<StoredGroupMessage>,
    /// The inbox that sent the delete. `None` when no inbox did: expiry
    /// cleanup, or a local removal of the row.
    pub deleted_by_inbox_id: Option<String>,
}

impl InternalEvent {
    /// Whether the event reports deleted messages, by any cause.
    pub(crate) fn is_message_deletion(&self) -> bool {
        matches!(self, Self::MessagesDeleted(_) | Self::MessagesExpired(_))
    }
}

/// Report stored consent changes: one event per changed key and entity the
/// app names it by, the legacy stream, and a wake for the outbox when a change
/// is owed. This is the only consent event emitter.
// implements: CONS-059, EVENT-001, EVENT-059
pub(crate) fn emit_consent_changes(
    ctx: &impl crate::context::XmtpSharedContext,
    events: &impl EventWriter<InternalEvent>,
    db: &impl DbQuery,
    changes: &[ConsentChange],
) -> Result<(), ConnectionError> {
    // One entry per key: the reported state before the first change, the value after the last.
    let mut net: Vec<(&ConsentKey, StoredConsentState, ConsentValue)> = vec![];
    for change in changes {
        match net.iter_mut().find(|(key, ..)| **key == change.key) {
            Some(entry) => entry.2 = change.value,
            None => net.push((&change.key, change.before, change.value)),
        }
    }
    let mut legacy = vec![];
    let mut state_changed = false;
    for (key, before, value) in net {
        let changed = before != value.state;
        state_changed |= changed;
        for (entity_type, entity, context) in app_entities(db, ctx.inbox_id(), key)? {
            if changed {
                let kind = match entity_type {
                    ConsentType::ConversationId => ConsentEntityKind::Conversation,
                    ConsentType::InboxId => ConsentEntityKind::Inbox,
                };
                let event = ClientEvent::ConsentChanged(ConsentChanged {
                    entity_kind: kind,
                    entity: entity.clone(),
                    state: match value.state {
                        StoredConsentState::Unknown => ConsentState::Unknown,
                        StoredConsentState::Allowed => ConsentState::Allowed,
                        StoredConsentState::Denied => ConsentState::Denied,
                    },
                });
                events.emit_with_context(Some(event), None, context);
            }
            legacy.push(PreferenceUpdate::Consent(StoredConsentRecord {
                entity_type,
                state: value.state,
                entity,
                consented_at_ns: value.consented_at_ns,
                owed_seq: 0,
            }));
        }
    }
    if state_changed {
        // implements: PUSH-260
        ctx.task_channels().mark_notification_changed();
    }
    if !legacy.is_empty() {
        events.emit(
            None,
            Some(InternalEvent::PreferencesChanged { updates: legacy }),
        );
    }
    if changes.iter().any(|change| change.marked) {
        events.emit(None, Some(InternalEvent::OutboxChanged));
    }
    Ok(())
}

/// The entities an app names for one changed key: an inbox, and
/// the DM the app lists with that inbox; a conversation only when
/// no held group reads another key. Never a virtual group.
fn app_entities(
    db: &impl DbQuery,
    own_inbox: &str,
    key: &ConsentKey,
) -> Result<Vec<(ConsentType, String, EventContext)>, ConnectionError> {
    let mut entities = vec![];
    let held = match key.entity_type {
        ConsentType::InboxId => {
            entities.push((
                ConsentType::InboxId,
                key.entity.clone(),
                EventContext::default(),
            ));
            let dm_id = DmMembers {
                member_one_inbox_id: own_inbox,
                member_two_inbox_id: key.entity.as_str(),
            };
            db.find_dm_by_dm_id(&dm_id.to_string())?
        }
        ConsentType::ConversationId => {
            let Some(id) = key.group_id() else {
                return Ok(entities);
            };
            let held = db.find_group(&id)?;
            if held.is_none() {
                entities.push((
                    ConsentType::ConversationId,
                    key.entity.clone(),
                    EventContext::default(),
                ));
            }
            held
        }
    };
    if let Some(group) = held
        && !group.conversation_type.is_virtual()
        && ConsentKey::of_group(&group, own_inbox).as_ref() == Some(key)
    {
        let context = dm_event_context(group.dm_id.as_deref());
        entities.push((ConsentType::ConversationId, hex::encode(group.id), context));
    }
    Ok(entities)
}

/// Report a stored root-key change: push re-registers, the legacy stream
/// receives the key, and the outbox wakes when the key is owed.
pub(crate) fn emit_root_key_changed(
    events: &impl EventWriter<InternalEvent>,
    db: &impl DbQuery,
    marked: bool,
) -> Result<(), xmtp_db::StorageError> {
    let preferences = StoredUserPreferences::load(db)?;
    let updates = preferences
        .hmac_key
        .map(|key| PreferenceUpdate::Hmac {
            key: key.as_slice().to_vec().into(),
            cycled_at_ns: preferences.hmac_key_cycled_at_ns.unwrap_or_default(),
        })
        .into_iter()
        .collect();
    events.emit(
        Some(ClientEvent::HmacKeysUpdated(HmacKeysUpdated)),
        Some(InternalEvent::PreferencesChanged { updates }),
    );
    if marked {
        events.emit(None, Some(InternalEvent::OutboxChanged));
    }
    Ok(())
}

pub(crate) fn dm_event_context(dm_id: Option<&str>) -> EventContext {
    EventContext {
        dm_identifier: dm_id.map(|id| id.as_bytes().to_vec()),
        ..Default::default()
    }
}

/// Emit the deletion of `messages` by the inbox `deleted_by_inbox_id`, or by
/// no inbox (`None`) for a local removal of the rows.
pub(crate) fn emit_deleted_messages(
    writer: &impl EventWriter<InternalEvent>,
    messages: Vec<StoredGroupMessage>,
    cause: DeletionCause,
    deleted_by_inbox_id: Option<String>,
    db: &impl DbQuery,
) -> Result<(), ConnectionError> {
    if messages.is_empty() {
        return Ok(());
    }
    for message in &messages {
        let group = db.find_group(&message.group_id)?;
        if group
            .as_ref()
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
        let context = dm_event_context(group.as_ref().and_then(|group| group.dm_id.as_deref()));
        writer.emit_with_context(Some(event), None, context);
    }
    // A delete of an already expired message must not deliver its body.
    // implements: META-051
    let now = xmtp_common::time::now_ns();
    let (expired, live): (Vec<_>, Vec<_>) = messages
        .into_iter()
        .partition(|message| message.expire_at_ns.is_some_and(|at| at <= now));
    if !live.is_empty() {
        writer.emit(
            None,
            Some(InternalEvent::MessagesDeleted(DeletedMessages {
                messages: live,
                deleted_by_inbox_id: deleted_by_inbox_id.clone(),
            })),
        );
    }
    if !expired.is_empty() {
        let expired = expired
            .into_iter()
            .map(|message| StoredGroupMessage {
                decrypted_message_bytes: Vec::new(),
                ..message
            })
            .collect();
        writer.emit(
            None,
            Some(InternalEvent::MessagesExpired(DeletedMessages {
                messages: expired,
                deleted_by_inbox_id,
            })),
        );
    }
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
        let group = db.find_group(&message.group_id)?;
        if group
            .as_ref()
            .is_some_and(|group| group.conversation_type.is_virtual())
        {
            continue;
        }
        let context = dm_event_context(group.as_ref().and_then(|group| group.dm_id.as_deref()));
        writer.emit_with_context(
            Some(ClientEvent::MessageExpired(MessageRef {
                group_id: message.group_id.to_vec(),
                message_id: message.id.clone(),
            })),
            None,
            context,
        );
    }
    writer.emit(
        None,
        Some(InternalEvent::MessagesExpired(DeletedMessages {
            messages,
            deleted_by_inbox_id: None,
        })),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{context::XmtpSharedContext, tester};
    use xmtp_db::{
        group::{GroupMembershipState, StoredGroup},
        prelude::*,
    };
    use xmtp_events::{EventBus, EventFilter, EventKind};
    use xmtp_proto::types::ConversationType;

    // verifies: EVENT-020
    #[xmtp_common::test(unwrap_try = true)]
    async fn stitched_dm_delete_and_expiry_reach_group_filter() {
        use crate::test::mock::generate_stored_msg;
        use xmtp_proto::types::Cursor;

        tester!(alix, disable_workers);
        let db = alix.context.db();
        let [ga, gb, gc] = [71, 72, 73].map(|id| GroupId::from([id; 16]));
        for (id, dm_id) in [(ga, "stitched"), (gb, "stitched"), (gc, "other")] {
            StoredGroup::builder()
                .id(id)
                .created_at_ns(xmtp_common::time::now_ns())
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id(alix.inbox_id())
                .conversation_type(ConversationType::Dm)
                .dm_id(Some(dm_id.to_string()))
                .build()?
                .store(&db)?;
        }
        let messages = [
            generate_stored_msg(Cursor(1), gb),
            generate_stored_msg(Cursor(2), gc),
        ];
        let bus = EventBus::<InternalEvent>::new();
        let mut filter = EventFilter::new([EventKind::MessageDeleted, EventKind::MessageExpired]);
        filter.group_ids = Some(vec![ga.to_vec()]);
        filter.dm_identifiers = vec![db.find_group(&ga)?.unwrap().dm_id.unwrap().into_bytes()];
        let public = bus.subscribe(filter, Some(10));
        emit_deleted_messages(
            &bus,
            messages.to_vec(),
            DeletionCause::DeletedLocally,
            None,
            &db,
        )?;
        emit_expired_messages(&bus, messages.to_vec(), &db)?;
        let events: Vec<_> = public
            .drain()
            .into_iter()
            .filter_map(|e| e.client)
            .collect();
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], ClientEvent::MessageDeleted(deleted)
            if deleted.group_id == gb.to_vec()));
        assert!(matches!(&events[1], ClientEvent::MessageExpired(expired)
            if expired.group_id == gb.to_vec()));
    }

    /// One event per changed key and app entity: an inbox change is also
    /// reported under the DM the app lists with that inbox, Restored too; a
    /// group record that a held DM does not read, a virtual group, and a new
    /// unknown record report nothing; the legacy stream gets every entity.
    // verifies: CONS-059, EVENT-001, EVENT-059
    #[xmtp_common::test(unwrap_try = true)]
    async fn consent_events() {
        use xmtp_db::consent_record::{ConsentChange, ConsentKey, ConsentValue};
        tester!(alix, disable_workers);
        let db = alix.context.db();
        let own = alix.inbox_id();
        let store = |id: u8, peer: Option<&str>, kind, state| {
            let mut pair = peer.map(|peer| [own.to_string(), peer.to_string()]);
            if let Some(pair) = &mut pair {
                pair.sort();
            }
            StoredGroup::builder()
                .id(GroupId::from([id; 16]))
                .created_at_ns(1)
                .membership_state(state)
                .added_by_inbox_id(own)
                .conversation_type(kind)
                .dm_id(pair.map(|pair| format!("dm:{}", pair.join(":"))))
                .build()
                .unwrap()
                .store(&db)
                .unwrap();
            GroupId::from([id; 16])
        };
        let restored = store(
            1,
            Some("peer-two"),
            ConversationType::Dm,
            GroupMembershipState::Restored,
        );
        let held_dm = store(
            2,
            Some("peer-three"),
            ConversationType::Dm,
            GroupMembershipState::Allowed,
        );
        let sync = store(
            3,
            None,
            ConversationType::Sync,
            GroupMembershipState::Allowed,
        );
        let change = |key: ConsentKey, before, state| ConsentChange {
            key,
            before,
            value: ConsentValue {
                state,
                consented_at_ns: 1,
            },
            marked: false,
        };
        let bus = EventBus::<InternalEvent>::new();
        let public = bus.subscribe(EventFilter::new([EventKind::ConsentChanged]), Some(16));
        let legacy = bus.subscribe(
            EventFilter::default()
                .with_internal(|event| matches!(event, InternalEvent::PreferencesChanged { .. })),
            Some(16),
        );
        let named = |events: Vec<xmtp_events::EventEnvelope<InternalEvent>>| -> Vec<String> {
            events
                .into_iter()
                .filter_map(|event| match event.client {
                    Some(ClientEvent::ConsentChanged(change)) => Some(change.entity),
                    _ => None,
                })
                .collect()
        };
        use StoredConsentState::*;
        for (changes, expected) in [
            (
                vec![change(ConsentKey::inbox("peer-one"), Unknown, Denied)],
                vec!["peer-one".to_string()],
            ),
            (
                vec![change(ConsentKey::inbox("peer-two"), Unknown, Denied)],
                vec!["peer-two".to_string(), hex::encode(restored)],
            ),
            (
                vec![change(ConsentKey::group(held_dm.as_ref()), Unknown, Denied)],
                vec![],
            ),
            (
                vec![change(ConsentKey::group(sync.as_ref()), Unknown, Denied)],
                vec![],
            ),
            (
                vec![change(ConsentKey::inbox("new"), Unknown, Unknown)],
                vec![],
            ),
        ] {
            emit_consent_changes(&alix.context, &bus, &db, &changes)?;
            assert_eq!(named(public.drain()), expected, "{changes:?}");
        }
        let entities: Vec<Vec<String>> = legacy
            .drain()
            .into_iter()
            .filter_map(|event| match event.internal {
                Some(InternalEvent::PreferencesChanged { updates }) => Some(
                    updates
                        .into_iter()
                        .filter_map(|update| match update {
                            PreferenceUpdate::Consent(record) => Some(record.entity),
                            PreferenceUpdate::Hmac { .. } => None,
                        })
                        .collect(),
                ),
                _ => None,
            })
            .collect();
        assert_eq!(
            entities,
            vec![
                vec!["peer-one".to_string()],
                vec!["peer-two".to_string(), hex::encode(restored)],
                vec!["new".to_string()],
            ]
        );
    }
}
