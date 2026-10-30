use super::DeviceSyncError;
use crate::{
    context::XmtpSharedContext, groups::MlsGroup, state_tx::state_write,
    subscriptions::internal::InternalEvent, worker::NeedsDbReconnect,
};
use futures::{Stream, StreamExt};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
pub use xmtp_archive::*;
use xmtp_configuration::LEGACY_GROUP_ID_BYTES;
use xmtp_db::{
    StorageError,
    TransactionOutcome::Continue,
    consent_record::{ConsentKey, ConsentWrite, Owed, parse_consent_save, write_consent},
    group::ConversationType,
    group_message::StoredGroupMessage,
    prelude::*,
};
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::device_sync::{
    BackupElement, backup_element::Element, consent_backup::ConsentSave,
};

use xmtp_events::{ArchiveRestored, ClientEvent, EventWriter};
#[derive(Default)]
struct ImportContext {
    changed: bool,
    /// The archive's consent records, applied after all its elements.
    consent: Vec<ConsentSave>,
    /// The archive's 16-byte DM groups, whatever their restore did.
    dm_groups: HashSet<GroupId>,
    /// Whether the client holds each group the archive's messages name as a
    /// sync or one-shot conversation.
    internal: HashMap<GroupId, bool>,
}

impl ImportContext {
    fn is_internal(&mut self, db: &impl DbQuery, group_id: GroupId) -> Result<bool, StorageError> {
        if let Some(internal) = self.internal.get(&group_id) {
            return Ok(*internal);
        }
        let internal = db
            .find_group(&group_id)?
            .is_some_and(|group| group.conversation_type.is_virtual());
        self.internal.insert(group_id, internal);
        Ok(internal)
    }
}

pub async fn insert_importer(
    importer: &mut ArchiveImporter,
    context: &impl XmtpSharedContext,
) -> Result<(), DeviceSyncError> {
    insert_elements(importer, context).await
}

// implements: ARCH-027
async fn insert_elements<S, E>(
    elements: &mut S,
    context: &impl XmtpSharedContext,
) -> Result<(), DeviceSyncError>
where
    S: Stream<Item = Result<BackupElement, E>> + Unpin,
    DeviceSyncError: From<E>,
{
    let mut import = ImportContext::default();
    let mut first_error = None;
    while let Some(element) = elements.next().await {
        let element = match element {
            Ok(element) => element,
            // A broken stream ends the import.
            Err(e) => {
                first_error.get_or_insert(DeviceSyncError::from(e));
                break;
            }
        };
        // A failed element does not stop the rest: a pre-8.0 archive lists
        // consent and later groups after a message whose group it left out.
        if let Err(e) = insert(element, context, &mut import) {
            if e.needs_db_reconnect() {
                return Err(e);
            }
            first_error.get_or_insert(e);
        }
    }
    // Consent after groups, even after a failed element.
    let consent = state_write(context.mls_storage(), |tx| {
        let storage = tx.storage();
        let db = storage.db();
        let changed = import_consent(
            &db,
            context.inbox_id(),
            &import,
            xmtp_common::time::now_ns(),
        )?;
        Ok::<_, StorageError>(Continue(changed))
    });
    match consent {
        Ok(changed) => import.changed |= changed.into_continued(),
        Err(e) if e.db_needs_connection() => return Err(e.into()),
        Err(e) => {
            first_error.get_or_insert(e.into());
        }
    }
    let result = first_error.map_or(Ok(()), Err);
    if import.changed {
        context.task_channels().mark_notification_changed();
        context.events().emit(
            Some(ClientEvent::ArchiveRestored(ArchiveRestored {
                complete: result.is_ok(),
            })),
            // Imported records are owed to the other installations.
            Some(InternalEvent::OutboxChanged),
        );
    }
    result
}

fn insert(
    element: BackupElement,
    context: &impl XmtpSharedContext,
    import: &mut ImportContext,
) -> Result<(), DeviceSyncError> {
    let Some(element) = element.element else {
        return Ok(());
    };

    match element {
        Element::Consent(save) => import.consent.push(save),
        // An archive carries no sync or one-shot conversation (ARCH-010), so
        // an import makes none and adds no message to one: device sync
        // applies only what the backend delivered.
        Element::Group(save)
            if ConversationType::try_from(save.conversation_type())
                .is_ok_and(|kind| kind.is_virtual()) => {}
        Element::Group(save) => {
            // Registered before the restore, so a re-import that finds the
            // group held combines the same rows.
            if save.dm_id.is_some()
                && save.id.len() == LEGACY_GROUP_ID_BYTES
                && let Ok(id) = GroupId::try_from(save.id.as_slice())
            {
                import.dm_groups.insert(id);
            }
            import.changed |= MlsGroup::restore_from_archive(context, &save)?;
        }
        Element::GroupMessage(message) => {
            let message: StoredGroupMessage = message.try_into()?;
            let db = context.db();
            if !import.is_internal(&db, message.group_id)? {
                import.changed |= message.store_or_ignore_changed(&db)?;
            }
        }
        _ => {}
    }

    Ok(())
}

/// Apply one archive's consent records, after its groups. When the archive
/// carries a 16-byte DM with a peer, its records for every 16-byte DM held
/// with that peer and its inbox record for the peer become one record. For
/// any other peer, a record naming a 16-byte DM held from elsewhere moves to
/// the peer and merges by time. Every other record merges as named. Returns
/// whether anything changed.
// implements: CONS-051, CONS-054, CONS-061
fn import_consent(
    db: &impl DbQuery,
    own_inbox: &str,
    import: &ImportContext,
    now_ns: i64,
) -> Result<bool, StorageError> {
    // The peers of the 16-byte DMs this archive carries.
    let mut carried = BTreeSet::new();
    for id in &import.dm_groups {
        if let Some((peer, _)) = ConsentKey::group(id.as_ref()).pre8_dm(db, own_inbox)? {
            carried.insert(peer);
        }
    }
    let mut plain: BTreeMap<ConsentKey, Vec<ConsentSave>> = BTreeMap::new();
    let mut pre8: BTreeMap<ConsentKey, Vec<ConsentSave>> = BTreeMap::new();
    for save in &import.consent {
        // An unusable record is skipped.
        let Some((key, _)) = parse_consent_save(save) else {
            continue;
        };
        // A record naming a held 16-byte DM is its peer's.
        let key = key.pre8_dm(db, own_inbox)?.map_or(key, |(peer, _)| peer);
        let rows = if carried.contains(&key) {
            pre8.entry(key)
        } else {
            plain.entry(key)
        };
        rows.or_default().push(save.clone());
    }
    for (peer, rows) in pre8 {
        let combined = pre8::pre8_peer_consent(&peer.entity, &rows);
        plain.insert(peer, combined.into_iter().collect());
    }
    let mut changed = false;
    for (key, saves) in plain {
        for save in saves {
            let Some((_, value)) = parse_consent_save(&save) else {
                continue;
            };
            let write = ConsentWrite::from_archive(value, now_ns);
            changed |= write_consent(db, &key, write, Owed::Mark)?.is_some();
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    #![allow(unused)]
    use super::*;
    use crate::groups::send_message_opts::SendMessageOpts;
    use crate::tester;
    use crate::utils::{LocalTester, Tester};
    use crate::worker::device_sync::{ArchiveOptions, BackupElementSelection};
    use crate::{builder::ClientBuilder, utils::test::wait_for_min_intents};
    use diesel::prelude::*;
    use futures::AsyncReadExt;
    use futures::io::{BufReader, Cursor};
    use std::{path::Path, sync::Arc};
    use xmtp_archive::exporter;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::group_message::MsgQueryArgs;
    use xmtp_db::{ConnectionExt, group::GroupMembershipState};
    use xmtp_db::{
        consent_record::StoredConsentRecord,
        group::StoredGroup,
        group_message::StoredGroupMessage,
        schema::{consent_records, group_messages, groups},
    };
    use xmtp_mls_common::{
        group::{DMMetadataOptions, GroupMetadataOptions},
        group_mutable_metadata::MessageDisappearingSettings,
    };
    use xmtp_proto::types::GroupId;

    /// Runs `on_first` once, when the export first writes past the archive
    /// header, which happens while the snapshot's read transaction is open.
    struct WriteDuringExport<F: FnMut()> {
        archive: Vec<u8>,
        on_first: Option<F>,
    }

    impl<F: FnMut()> std::io::Write for WriteDuringExport<F> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.archive.len() >= 2 + xmtp_archive::NONCE_SIZE {
                self.on_first.take().into_iter().for_each(|mut f| f());
            }
            self.archive.write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// A selected group whose mutable metadata cannot be decoded fails the
    /// export, rather than exporting the group without that value. The
    /// malformed admin list is committed locally through openmls, bypassing
    /// the commit validation that keeps it out of real groups.
    // verifies: ARCH-017
    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_export_fails_on_undecodable_group_metadata() {
        use openmls::{
            component::ComponentData,
            group::MlsGroup,
            messages::proposals::{AppDataUpdateProposal, Proposal},
        };
        use openmls_traits::OpenMlsProvider;
        use xmtp_archive::{ArchiveError, UnreadableGroup};
        use xmtp_mls_common::app_data::component_id::ComponentId;

        tester!(alix, disable_workers);
        let conversation = alix.create_group(None, None)?;
        let provider = alix.context.mls_provider();
        let signer = &alix.context.identity().installation_keys;
        let mut group = MlsGroup::load(
            alix.context.mls_storage(),
            &conversation.group_id.to_openmls(),
        )??;
        let malformed = vec![0xff; 3];
        let mut stage = crate::groups::mls_ext::commit_without_group_info(
            &mut group,
            provider.storage(),
            |builder| {
                builder.add_proposal(Proposal::AppDataUpdate(Box::new(
                    AppDataUpdateProposal::update(
                        ComponentId::ADMIN_LIST.as_u16(),
                        malformed.clone(),
                    ),
                )))
            },
        )?;
        let mut updater = stage.app_data_dictionary_updater();
        updater.set(ComponentData::from_parts(
            ComponentId::ADMIN_LIST.as_u16(),
            malformed.into(),
        ));
        stage.with_app_data_dictionary_updates(updater.changes());
        stage
            .build(provider.rand(), provider.crypto(), signer, |_| true)?
            .stage_commit(&provider)?;
        group.merge_pending_commit(&provider)?;

        let opts = ArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![BackupElementSelection::Messages],
            exclude_disappearing_messages: false,
        };
        let key = xmtp_common::rand_vec::<32>();
        let failure = exporter::export(opts, alix.db(), &key, Vec::new());
        assert!(
            matches!(
                failure,
                Err(ArchiveError::UnreadableGroup {
                    group_id,
                    source: UnreadableGroup::MutableMetadata(_),
                }) if group_id == conversation.group_id
            ),
            "export dropped an undecodable metadata value: {failure:?}"
        );
    }

    /// An archive holds no group key. A keyed group's root never reaches it,
    /// and a commit-log signer that a legacy group's dictionary still holds
    /// is dropped from the exported attributes. The signer is committed
    /// locally through openmls, because new groups never write one.
    // verifies: GMOD-042
    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_export_holds_no_group_keys() {
        use openmls::{
            component::ComponentData,
            group::MlsGroup,
            messages::proposals::{AppDataUpdateProposal, Proposal},
        };
        use openmls_traits::OpenMlsProvider;
        use prost::Message;
        use xmtp_mls_common::app_data::{component_id::ComponentId, group_root::read_group_root};

        tester!(alix, disable_workers);
        let conversation = alix.create_group(None, None)?;
        conversation
            .send_message(b"archived", Default::default())
            .await?;
        let root = conversation
            .with_group_snapshot(|mls_group| Ok(read_group_root(mls_group.extensions())))???;
        let provider = alix.context.mls_provider();
        let signer = &alix.context.identity().installation_keys;
        let mut group = MlsGroup::load(
            alix.context.mls_storage(),
            &conversation.group_id.to_openmls(),
        )??;
        let commit_log_signer = xmtp_common::rand_vec::<32>();
        let mut stage = crate::groups::mls_ext::commit_without_group_info(
            &mut group,
            provider.storage(),
            |builder| {
                builder.add_proposal(Proposal::AppDataUpdate(Box::new(
                    AppDataUpdateProposal::update(
                        ComponentId::COMMIT_LOG_SIGNER.as_u16(),
                        commit_log_signer.clone(),
                    ),
                )))
            },
        )?;
        let mut updater = stage.app_data_dictionary_updater();
        updater.set(ComponentData::from_parts(
            ComponentId::COMMIT_LOG_SIGNER.as_u16(),
            commit_log_signer.clone().into(),
        ));
        stage.with_app_data_dictionary_updates(updater.changes());
        stage
            .build(provider.rand(), provider.crypto(), signer, |_| true)?
            .stage_commit(&provider)?;
        group.merge_pending_commit(&provider)?;

        let opts = ArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            exclude_disappearing_messages: false,
        };
        let key = xmtp_common::rand_vec::<32>();
        let mut archive = Vec::new();
        exporter::export(opts, alix.db(), &key, &mut archive)?;
        let reader = Box::pin(BufReader::new(Cursor::new(archive)));
        let elements: Vec<_> = ArchiveImporter::load(reader, &key)
            .await?
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<_, _>>()?;

        let mut saved = None;
        let mut plaintext = Vec::new();
        for element in &elements {
            plaintext.extend(element.encode_to_vec());
            if let Some(Element::Group(group)) = &element.element
                && group.id == conversation.group_id.to_vec()
            {
                saved = Some(group.clone());
            }
        }
        let attributes = saved?.mutable_metadata?.attributes;
        assert!(
            attributes.contains_key("group_name"),
            "the archive lost the group's attributes: {attributes:?}"
        );
        assert!(!attributes.contains_key("_commit_log_signer"));
        let contains = |needle: &[u8]| plaintext.windows(needle.len()).any(|w| w == needle);
        for (secret, encoded) in crate::utils::test::group_secret_encodings(&root) {
            assert!(!contains(&encoded), "the archive holds the {secret}");
        }
        assert!(
            !contains(&commit_log_signer),
            "the archive holds the signer"
        );
        assert!(
            !contains(hex::encode(&commit_log_signer).as_bytes()),
            "the archive holds the signer as hex"
        );
        assert!(!contains(b"_commit_log_signer"));
    }

    /// With MESSAGES selected, a restore into a new installation holds every
    /// eligible conversation whatever its creation time or messages: an empty
    /// group, an empty DM, a group whose messages all precede the window, and
    /// an old group with its in-window message. Consent edits and insertions
    /// committed on another connection while the export is streaming are not
    /// in the archive: the export is one snapshot. On wasm, whose single
    /// connection the export holds, such writes are refused instead.
    // verifies: ARCH-007, ARCH-017
    // verifies: EVENT-004
    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_snapshot_is_complete() {
        use xmtp_db::consent_record::{ConsentState, ConsentType};
        use xmtp_proto::xmtp::device_sync::{
            backup_element::Element, consent_backup::ConsentStateSave,
        };

        tester!(alix, persistent_db, disable_workers);
        tester!(bo, disable_workers);
        let empty_group = alix.create_group(None, None)?;
        let empty_dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
        let outside = alix.create_group(None, None)?;
        let outside_message = outside.send_message_optimistic(b"before", Default::default())?;
        let old = alix.create_group(None, None)?;
        let start_ns = xmtp_common::time::now_ns();
        // The window's start is exclusive, and wasm clocks tick in whole
        // milliseconds, so the in-window message must be sent a tick later.
        xmtp_common::time::sleep(std::time::Duration::from_millis(2)).await;
        // Large enough that the archive reaches the sink before consent is read.
        let in_window = old.send_message_optimistic(&[7; 512 * 1024], Default::default())?;
        let consent = |entity: &str, state| {
            StoredConsentRecord::new(ConsentType::InboxId, state, entity.to_string())
        };
        consent("carol", ConsentState::Allowed).merge_into(&alix.db())?;

        let opts = ArchiveOptions {
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            start_ns: Some(start_ns),
            end_ns: None,
            exclude_disappearing_messages: false,
        };
        let key = vec![7; 32];
        let writer = alix.db();
        let mut sink = WriteDuringExport {
            archive: vec![],
            on_first: Some(|| {
                let written = consent("carol", ConsentState::Denied)
                    .merge_into(&writer)
                    .and_then(|_| consent("dave", ConsentState::Allowed).merge_into(&writer));
                // Native writers on other connections commit during the export;
                // wasm's single connection is held by the export and refuses
                // them, so neither can reach the snapshot.
                #[cfg(not(target_arch = "wasm32"))]
                written.unwrap();
                #[cfg(target_arch = "wasm32")]
                assert!(written.is_err(), "wasm wrote during the export");
            }),
        };
        exporter::export(opts, alix.db(), &key, &mut sink)?;
        assert!(
            sink.on_first.is_none(),
            "the export never wrote mid-snapshot"
        );
        let archive = sink.archive;

        let reader = Box::pin(BufReader::new(Cursor::new(archive.clone())));
        let consent: Vec<_> = ArchiveImporter::load(reader, &key)
            .await?
            .filter_map(|e| async move {
                match e.ok()?.element? {
                    Element::Consent(c) if c.entity == "carol" || c.entity == "dave" => Some(c),
                    _ => None,
                }
            })
            .collect()
            .await;
        assert!(
            matches!(
                consent.as_slice(),
                [c] if c.entity == "carol" && c.state == ConsentStateSave::Allowed as i32
            ),
            "the export saw writes committed after its snapshot: {consent:?}"
        );

        tester!(alix2, from: alix, disable_workers);
        let events = alix2.context.events().subscribe(
            xmtp_events::EventFilter::new([
                xmtp_events::EventKind::ArchiveRestored,
                xmtp_events::EventKind::ConversationJoined,
                xmtp_events::EventKind::MessageReceived,
                xmtp_events::EventKind::ConsentChanged,
            ]),
            Some(64),
        );
        let reader = Box::pin(BufReader::new(Cursor::new(archive)));
        let mut importer = ArchiveImporter::load(reader, &key).await?;
        insert_importer(&mut importer, &alix2.context).await?;

        let db = alix2.db();
        for group in [&empty_group, &empty_dm, &outside, &old] {
            assert_eq!(
                db.find_group(&group.group_id)??.membership_state,
                GroupMembershipState::Restored
            );
        }
        assert!(db.get_group_message(&in_window)?.is_some());
        assert!(db.get_group_message(&outside_message)?.is_none());
        assert!(
            matches!(events.drain().as_slice(), [xmtp_events::EventEnvelope { client: Some(ClientEvent::ArchiveRestored(restored)), .. }] if restored.complete)
        );
    }

    // verifies: EVENT-001, EVENT-017
    #[xmtp_common::test(unwrap_try = true)]
    async fn partial_import_reports_incomplete_after_a_stored_change() {
        tester!(alix, disable_workers);
        let events = alix.context.events().subscribe(
            xmtp_events::EventFilter::new([xmtp_events::EventKind::ArchiveRestored]),
            Some(4),
        );
        let record = StoredConsentRecord::new(
            xmtp_db::consent_record::ConsentType::InboxId,
            xmtp_db::consent_record::ConsentState::Allowed,
            "failed-import".into(),
        );
        let element = BackupElement {
            element: Some(Element::Consent(record.into())),
        };
        let mut elements =
            futures::stream::iter([Ok(element), Err(std::io::Error::other("archive cut"))]);
        assert!(insert_elements(&mut elements, &alix.context).await.is_err());
        assert!(
            alix.db()
                .consent_row(&xmtp_db::consent_record::ConsentKey::inbox("failed-import"))?
                .is_some()
        );
        assert!(matches!(
            events.drain().as_slice(),
            [xmtp_events::EventEnvelope {
                client: Some(ClientEvent::ArchiveRestored(restored)), ..
            }] if !restored.complete
        ));
    }

    // verifies: EVENT-004
    #[xmtp_common::test(unwrap_try = true)]
    async fn failed_import_suppresses_events_for_all_stored_elements() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let group = alix.create_group(None, None)?;
        let message_id = group.send_message_optimistic(b"archive message", Default::default())?;
        StoredConsentRecord::new(
            xmtp_db::consent_record::ConsentType::InboxId,
            xmtp_db::consent_record::ConsentState::Denied,
            bo.inbox_id().into(),
        )
        .merge_into(&alix.db())?;
        let key = vec![9; 32];
        let mut archive = vec![];
        exporter::export(
            ArchiveOptions {
                elements: vec![
                    BackupElementSelection::Messages,
                    BackupElementSelection::Consent,
                ],
                start_ns: None,
                end_ns: None,
                exclude_disappearing_messages: false,
            },
            alix.db(),
            &key,
            &mut archive,
        )?;
        let importer =
            ArchiveImporter::load(Box::pin(BufReader::new(Cursor::new(archive))), &key).await?;
        let mut elements = importer
            .map(|item| item.map_err(DeviceSyncError::from))
            .chain(futures::stream::iter([Err(DeviceSyncError::from(
                std::io::Error::other("archive cut"),
            ))]));
        tester!(restored, from: alix, disable_workers);
        let events = restored.context.events().subscribe(
            xmtp_events::EventFilter::new([
                xmtp_events::EventKind::ArchiveRestored,
                xmtp_events::EventKind::ConversationJoined,
                xmtp_events::EventKind::MessageReceived,
                xmtp_events::EventKind::ConsentChanged,
            ]),
            Some(64),
        );
        assert!(
            insert_elements(&mut elements, &restored.context)
                .await
                .is_err()
        );
        assert!(restored.db().find_group(&group.group_id)?.is_some());
        assert!(restored.db().get_group_message(&message_id)?.is_some());
        assert!(
            restored
                .db()
                .consent_row(&xmtp_db::consent_record::ConsentKey::inbox(bo.inbox_id()))?
                .is_some()
        );
        assert!(
            matches!(events.drain().as_slice(), [xmtp_events::EventEnvelope { client: Some(ClientEvent::ArchiveRestored(restored)), .. }] if !restored.complete)
        );
    }
    /// The greater record wins, and a time more than a day past the import
    /// is stored at that limit.
    // verifies: CONS-053, CONS-054
    #[xmtp_common::test(unwrap_try = true)]
    async fn archived_consent_keeps_the_latest_record() {
        tester!(alix, disable_workers);
        let soon = xmtp_common::time::now_ns() + xmtp_common::NS_IN_HOUR;
        let records = [
            (xmtp_db::consent_record::ConsentState::Denied, i64::MAX),
            (xmtp_db::consent_record::ConsentState::Allowed, 10),
            (xmtp_db::consent_record::ConsentState::Allowed, soon),
            (xmtp_db::consent_record::ConsentState::Denied, 20),
        ];
        let mut elements = futures::stream::iter(records.map(|(state, consented_at_ns)| {
            let record = StoredConsentRecord {
                entity_type: xmtp_db::consent_record::ConsentType::InboxId,
                state,
                entity: "archived-inbox".into(),
                consented_at_ns,
                owed_seq: 0,
            };
            Ok::<_, std::io::Error>(BackupElement {
                element: Some(Element::Consent(record.into())),
            })
        }));
        insert_elements(&mut elements, &alix.context).await?;
        let limit = xmtp_common::time::now_ns() + xmtp_common::NS_IN_DAY;
        let stored = alix
            .db()
            .consent_row(&xmtp_db::consent_record::ConsentKey::inbox(
                "archived-inbox",
            ))?
            .unwrap();
        assert_eq!(stored.state, xmtp_db::consent_record::ConsentState::Denied);
        assert!(stored.consented_at_ns > soon && stored.consented_at_ns <= limit);
    }

    /// An import that only moves a stored record's time still stored an
    /// element, so it reports a restore.
    // verifies: EVENT-001
    #[xmtp_common::test(unwrap_try = true)]
    async fn archived_consent_time_move_reports_a_restore() {
        tester!(alix, disable_workers);
        let events = alix.context.events().subscribe(
            xmtp_events::EventFilter::new([xmtp_events::EventKind::ArchiveRestored]),
            Some(4),
        );
        let record = |consented_at_ns| StoredConsentRecord {
            entity_type: xmtp_db::consent_record::ConsentType::InboxId,
            state: xmtp_db::consent_record::ConsentState::Allowed,
            entity: "time-move".into(),
            consented_at_ns,
            owed_seq: 0,
        };
        assert!(record(10).merge_into(&alix.db())?.is_some());
        let mut elements = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
            element: Some(Element::Consent(record(30).into())),
        })]);
        insert_elements(&mut elements, &alix.context).await?;
        let stored = alix
            .db()
            .consent_row(&xmtp_db::consent_record::ConsentKey::inbox("time-move"))?
            .unwrap();
        assert_eq!(stored.consented_at_ns, 30);
        assert!(matches!(
            events.drain().as_slice(),
            [xmtp_events::EventEnvelope {
                client: Some(ClientEvent::ArchiveRestored(restored)), ..
            }] if restored.complete
        ));
    }

    /// An import that changes consent wakes the notification runner, which
    /// removes the topic of the conversation the archive denies.
    // verifies: PUSH-260
    #[xmtp_common::test(unwrap_try = true)]
    async fn archived_consent_wakes_notification_sync() {
        use crate::worker::{notifications::task_hash, notifications::tests::*, tasks::TaskWorker};
        use xmtp_common::{StreamHandle, time::Duration, time::timeout, wait_for_eq};
        let (client, peer) = support::client().await;
        let group = client.create_group(None, None)?;
        client.enable_notifications(config()).await?;
        let mut task_worker = TaskWorker::new(client.context.clone());
        let runner = xmtp_common::spawn(None, async move { task_worker.run().await });
        // Wait until the runner is idle, so only a wake runs it within the hour.
        let idle = || async {
            let tasks = client.db().get_tasks().unwrap();
            let task = tasks
                .iter()
                .find(|task| task.data_hash == task_hash().as_ref());
            let soon = xmtp_common::time::now_ns() + xmtp_common::NS_IN_MIN;
            task.is_some_and(|task| task.next_attempt_at_ns > soon)
        };
        timeout(Duration::from_secs(5), wait_for_eq(idle, true)).await??;
        let topic = xmtp_proto::types::Topic::new_group_message(group.group_id).cloned_vec();
        let holds = || async { peer.state.lock().subscriptions.contains_key(&topic) };
        assert!(holds().await);

        let revision = client.context.task_channels().notification_revision();
        let denied = StoredConsentRecord {
            entity_type: xmtp_db::consent_record::ConsentType::ConversationId,
            state: xmtp_db::consent_record::ConsentState::Denied,
            entity: hex::encode(group.group_id),
            consented_at_ns: xmtp_common::time::now_ns(),
            owed_seq: 0,
        };
        let mut elements = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
            element: Some(Element::Consent(denied.into())),
        })]);
        insert_elements(&mut elements, &client.context).await?;
        assert!(client.context.task_channels().notification_revision() > revision);
        timeout(Duration::from_secs(5), wait_for_eq(holds, false)).await??;
        runner.abort_handle().end();
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_timestamp_keeps_a_message_received_during_import() {
        tester!(alix, disable_workers);
        let group = alix.create_group(None, None)?;
        let save = xmtp_proto::xmtp::device_sync::group_backup::GroupSave {
            id: group.group_id.to_vec(),
            last_message_ns: Some(0),
            ..Default::default()
        };

        group.send_message_optimistic(b"message during import", Default::default())?;
        let current = alix.db().find_group(&group.group_id)??.last_message_ns;
        assert!(current > Some(0));
        MlsGroup::restore_from_archive(&alix.context, &save)?;
        assert_eq!(
            alix.db().find_group(&group.group_id)??.last_message_ns,
            current
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_archive_timestamps() {
        tester!(alix, disable_workers);
        tester!(alix2, from: alix);
        tester!(bo, disable_workers);

        let alix_group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        alix_group.send_message(b"hi", Default::default()).await?;

        alix2.sync_welcomes().await?;
        bo.sync_welcomes().await?;

        let alix2_group = alix2.group(&alix_group.group_id)?;
        let bo_group = bo.group(&alix_group.group_id)?;

        alix2_group.sync().await?;
        bo_group.sync().await?;

        // We want to send this message so that alix's group timestamp gets ahead of alix2.
        alix_group
            .send_message(b"Hello again", Default::default())
            .await?;

        let key = vec![7; 32];
        let opts = ArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            exclude_disappearing_messages: false,
        };
        let export = {
            let mut file = vec![];
            exporter::export(opts, alix.db(), &key, &mut file)?;
            file
        };

        tester!(alix3, from: alix);

        // Now we will have alix2 and alix3 import the archives.
        // One installation has the group already, one does not.
        let reader = Box::pin(BufReader::new(Cursor::new(export.clone())));
        let mut importer = ArchiveImporter::load(reader, &key).await?;
        insert_importer(&mut importer, &alix2.context).await?;

        let reader = Box::pin(BufReader::new(Cursor::new(export)));
        let mut importer = ArchiveImporter::load(reader, &key).await?;
        insert_importer(&mut importer, &alix3.context).await?;

        let alix_timestamp = alix
            .db()
            .find_group(&alix_group.group_id)??
            .last_message_ns?;
        let alix2_timestamp = alix2
            .db()
            .find_group(&alix_group.group_id)??
            .last_message_ns?;
        let alix3_timestamp = alix3
            .db()
            .find_group(&alix_group.group_id)??
            .last_message_ns?;

        // Alix2's older timestamp on the existing group should be updated.
        assert_eq!(alix2_timestamp, alix_timestamp);
        // Alix3's timestamp should equal alix's timestamp.
        assert_eq!(alix3_timestamp, alix_timestamp);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_dm_archive() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);

        let alix_bo_dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
        let archived_message_id = alix_bo_dm
            .send_message(b"old group", Default::default())
            .await?;

        let timestamp = alix
            .db()
            .find_group(&alix_bo_dm.group_id)??
            .last_message_ns?;

        let key = vec![7; 32];
        let opts = ArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            exclude_disappearing_messages: false,
        };
        let export = {
            let mut file = vec![];

            exporter::export(opts, alix.db(), &key, &mut file)?;
            file
        };

        tester!(alix2, from: alix);
        let reader = Box::pin(BufReader::new(Cursor::new(export)));
        let mut importer = ArchiveImporter::load(reader, &key).await?;
        insert_importer(&mut importer, &alix2.context).await?;

        // Imported history has no joined MLS state for this installation.
        // Receipt may retain the old prefix, but processing must wait for a Welcome.
        let restored = alix2.group(&alix_bo_dm.group_id)?;
        let restored_topic = xmtp_db::incoming_envelope::StreamTopic::group(alix_bo_dm.group_id);
        let before_join = alix2.db().topic_progress(&restored_topic)?;
        crate::mls_store::MlsStore::new(alix2.context.clone())
            .receive_topics_once(
                &[xmtp_proto::types::Topic::new_group_message(
                    alix_bo_dm.group_id,
                )],
                alix2
                    .context
                    .incoming_runtime()
                    .policy()
                    .incoming_limits(xmtp_db::incoming_envelope::NetworkEntityKind::Group),
            )
            .await?;
        let received_before_join = alix2.db().topic_progress(&restored_topic)?;
        assert!(received_before_join.received > before_join.processed);
        assert!(matches!(
            restored.process_pending_group_head(None)?,
            crate::groups::mls_sync::GroupHeadOutcome::Inactive
        ));
        assert_eq!(
            alix2.db().topic_progress(&restored_topic)?.processed,
            before_join.processed
        );
        assert!(
            !alix2
                .db()
                .pending_states_through(&restored_topic, received_before_join.received)?
                .is_empty()
        );

        let alix2_bo_dm = alix2.find_or_create_dm(bo.inbox_id(), None).await?;
        assert_ne!(alix_bo_dm.group_id, alix2_bo_dm.group_id);
        let mut msgs = alix2_bo_dm.find_messages(&MsgQueryArgs::default())?;
        assert_eq!(msgs.len(), 2);
        assert!(
            msgs.iter()
                .any(|m| m.decrypted_message_bytes == b"old group")
        );

        // assert_eq!(alix2_bo_dm.test_last_message_bytes().await??, b"old group");

        let timestamp2 = alix2
            .db()
            .find_group(&alix_bo_dm.group_id)??
            .last_message_ns?;
        assert_eq!(timestamp, timestamp2);

        let live_message_id = alix2_bo_dm
            .send_message(b"hi bo", Default::default())
            .await?;

        bo.sync_all_welcomes_and_groups(None).await?;
        let bo_alix2_dm = bo.group(&alix2_bo_dm.group_id)?;
        assert_eq!(bo_alix2_dm.test_last_message_bytes().await??, b"hi bo");

        // Ordinary sync must add the new installation to the original DM too.
        alix2.sync_all_welcomes_and_groups(None).await?;
        let rejoined_original = alix2.group(&alix_bo_dm.group_id)?;
        assert!(rejoined_original.is_active()?);
        let stitched = alix2_bo_dm.find_messages(&MsgQueryArgs::default())?;
        assert_eq!(stitched.len(), 4);
        let application_ids: Vec<_> = stitched
            .iter()
            .filter(|message| message.kind == xmtp_db::group_message::GroupMessageKind::Application)
            .map(|message| message.id.clone())
            .collect();
        assert_eq!(application_ids, vec![archived_message_id, live_message_id]);
        assert_eq!(alix2.find_groups(Default::default())?.len(), 1);
        let bo_original = bo.group(&alix_bo_dm.group_id)?;
        rejoined_original.test_can_talk_with(&bo_original).await?;
        bo_original.test_can_talk_with(&rejoined_original).await?;
    }

    // verifies: ARCH-015, ARCH-020
    #[xmtp_common::test(unwrap_try = true)]
    async fn foreign_archive_preserves_dm_pair_and_history() {
        use crate::groups::GroupError;
        use xmtp_db::{
            consent_record::{ConsentState, ConsentType},
            group::{DmIdExt, QueryGroup},
            readd_status::ReaddStatus,
        };
        use xmtp_proto::api::HasStats;

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        tester!(charlie, disable_workers);
        tester!(dana, disable_workers);
        let source = alix.find_or_create_dm(bo.inbox_id(), None).await?;
        let alix_id = source
            .send_message(b"from alix", Default::default())
            .await?;
        bo.sync_welcomes().await?;
        let bo_group = bo.group(&source.group_id)?;
        let bo_id = bo_group
            .send_message(b"from bo", Default::default())
            .await?;
        source.sync().await?;
        let archived_dm_id = alix
            .db()
            .find_group(&source.group_id)?
            .expect("source DM")
            .dm_id
            .expect("DM pair");
        assert!(!archived_dm_id.contains(charlie.inbox_id()));

        let key = vec![0x38; 32];
        let opts = ArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![BackupElementSelection::Messages],
            exclude_disappearing_messages: false,
        };
        let mut export = vec![];
        exporter::export(opts.clone(), alix.db(), &key, &mut export)?;
        let reader = Box::pin(BufReader::new(Cursor::new(export)));
        let mut importer = ArchiveImporter::load(reader, &key).await?;
        insert_importer(&mut importer, &charlie.context).await?;

        let stored = charlie
            .db()
            .find_group(&source.group_id)?
            .expect("Restored DM");
        assert_eq!(stored.membership_state, GroupMembershipState::Restored);
        assert_eq!(stored.dm_id.as_deref(), Some(archived_dm_id.as_str()));
        assert_eq!(archived_dm_id.other_inbox_id(charlie.inbox_id()), None);
        let restored = charlie.group(&source.group_id)?;
        assert_eq!(
            restored
                .metadata()
                .await?
                .dm_members
                .expect("stub pair")
                .to_string(),
            archived_dm_id
        );
        assert!(!restored.is_active()?);
        let stats = charlie.context.api().api_client.mls_stats();
        stats.clear();
        assert!(matches!(
            restored.send_message(b"inactive", Default::default()).await,
            Err(GroupError::GroupInactive)
        ));
        assert!(matches!(
            restored.sync().await,
            Err(GroupError::GroupInactive)
        ));
        let Err(sync_summary) = restored.sync_with_conn().await else {
            panic!("Restored sync_with_conn must reject the request");
        };
        assert!(matches!(
            sync_summary.other.as_deref(),
            Some(GroupError::GroupInactive)
        ));
        assert!(matches!(
            restored.receive().await,
            Err(GroupError::GroupInactive)
        ));
        assert!(matches!(
            charlie.client.sync_all_groups(vec![restored.clone()]).await,
            Err(GroupError::GroupInactive)
        ));
        assert_eq!(stats.publish.get_count(), 0);
        let joined = charlie.create_group(None, None)?;
        joined.add_members(&[dana.inbox_id()]).await?;
        dana.sync_welcomes().await?;
        let dana_group = dana.group(&joined.group_id)?;
        let incoming_id = dana_group
            .send_message(b"joined group", Default::default())
            .await?;
        let Err(GroupError::Sync(summary)) = charlie
            .client
            .sync_all_groups(vec![restored.clone(), joined])
            .await
        else {
            panic!("mixed explicit sync must report the inactive group");
        };
        assert!(matches!(
            summary.other.as_deref(),
            Some(GroupError::GroupInactive)
        ));
        assert!(charlie.db().get_group_message(&incoming_id)?.is_some());
        let sweep = charlie.client.sync_all_welcomes_and_groups(None).await?;
        assert_eq!(sweep.num_eligible, 1);
        assert_eq!(sweep.num_synced, 1);
        assert!(!restored.is_active()?);
        let db = charlie.db();
        // Even an allowed archived DM has no authority for remote recovery.
        StoredConsentRecord::new(
            ConsentType::ConversationId,
            ConsentState::Allowed,
            hex::encode(source.group_id),
        )
        .merge_into(&db)?;
        let excludes_publish = db
            .get_conversation_ids_for_remote_log_publish()?
            .iter()
            .all(|group| group.id != source.group_id);
        let excludes_download = db
            .get_conversation_ids_for_remote_log_download()?
            .iter()
            .all(|group| group.id != source.group_id);
        let excludes_fork_check = db
            .get_conversation_ids_for_fork_check()?
            .iter()
            .all(|id| id.as_slice() != source.group_id.as_ref());
        db.set_group_commit_log_forked_status(&source.group_id, Some(true))?;
        let excludes_requesting_readds = db
            .get_conversation_ids_for_requesting_readds()?
            .iter()
            .all(|group| group.group_id != source.group_id);
        ReaddStatus {
            group_id: source.group_id,
            installation_id: vec![0x42; 32],
            requested_at_sequence_id: Some(1),
            responded_at_sequence_id: None,
        }
        .store(&db)?;
        let excludes_responding_readds = db
            .get_conversation_ids_for_responding_readds()?
            .iter()
            .all(|group| group.group_id != source.group_id);
        assert_eq!(
            [
                excludes_publish,
                excludes_download,
                excludes_fork_check,
                excludes_requesting_readds,
                excludes_responding_readds,
            ],
            [true; 5],
            "Restored DM must be absent from every recovery selector"
        );
        for (id, sender, bytes) in [
            (&alix_id, alix.inbox_id(), b"from alix".as_slice()),
            (&bo_id, bo.inbox_id(), b"from bo".as_slice()),
        ] {
            let message = charlie.db().get_group_message(id)?.expect("pair history");
            assert_eq!(message.sender_inbox_id, sender);
            assert_eq!(message.decrypted_message_bytes, bytes);
        }

        let mut reexport = vec![];
        exporter::export(opts, charlie.db(), &key, &mut reexport)?;
        let reader = Box::pin(BufReader::new(Cursor::new(reexport)));
        let mut exported = ArchiveImporter::load(reader, &key).await?;
        while let Some(element) = exported.next().await {
            match element?.element {
                Some(Element::Group(group)) => assert_ne!(group.id, source.group_id.as_ref()),
                Some(Element::GroupMessage(message)) => {
                    assert_ne!(message.group_id, source.group_id.as_ref());
                }
                _ => {}
            }
        }

        let reopened = crate::builder::ClientBuilder::from_client(charlie.client.clone())
            .with_disable_workers(true)
            .build()
            .await?;
        let reopened_group = reopened.group(&source.group_id)?;
        assert_eq!(
            reopened_group.dm_id.as_deref(),
            Some(archived_dm_id.as_str())
        );
        assert!(!reopened_group.is_active()?);
        assert!(reopened.db().get_group_message(&alix_id)?.is_some());
        assert!(reopened.db().get_group_message(&bo_id)?.is_some());
    }

    // verifies: ARCH-013
    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_import_leaves_a_known_message_unchanged() {
        tester!(alix, disable_workers);
        let group = alix.create_group(None, None)?;
        let known_id = group.send_message(b"known", Default::default()).await?;
        let known = alix
            .db()
            .get_group_message(&known_id)?
            .expect("known message");
        let mut save: xmtp_proto::xmtp::device_sync::message_backup::GroupMessageSave =
            known.clone().into();
        save.sender_inbox_id = hex::encode([0x43; 32]);
        save.decrypted_message_bytes = b"ignored replacement".to_vec();
        let mut duplicate = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
            element: Some(Element::GroupMessage(save)),
        })]);
        insert_elements(&mut duplicate, &alix.context).await?;
        assert_eq!(alix.db().get_group_message(&known_id)?, Some(known));
    }

    // verifies: ARCH-019, ARCH-009
    #[xmtp_common::test(unwrap_try = true)]
    async fn imported_message_keeps_expiry_and_exclusion() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let source = alix.create_group(
            None,
            Some(GroupMetadataOptions {
                message_disappearing_settings: Some(MessageDisappearingSettings::new(
                    1,
                    xmtp_common::NS_IN_HOUR,
                )),
                ..Default::default()
            }),
        )?;
        let message_id = source
            .send_message(b"disappearing archive content", Default::default())
            .await?;
        let source_message = alix
            .db()
            .get_group_message(&message_id)?
            .expect("sent message");
        assert!(
            source_message
                .expire_at_ns
                .is_some_and(|deadline| deadline > xmtp_common::time::now_ns()),
            "the source message must still be eligible for export"
        );

        source
            .remove_conversation_message_disappearing_settings()
            .await?;
        let permanent_id = source
            .send_message(b"permanent archive content", Default::default())
            .await?;
        let permanent = alix
            .db()
            .get_group_message(&permanent_id)?
            .expect("permanent source");
        assert_eq!(
            (permanent.expire_at_ns, permanent.expiry_unknown),
            (None, false)
        );

        let key = [0x4a; 32];
        let options = ArchiveOptions {
            elements: vec![BackupElementSelection::Messages],
            start_ns: None,
            end_ns: None,
            exclude_disappearing_messages: false,
        };
        let mut first_archive = Vec::new();
        exporter::export(options.clone(), alix.db(), &key, &mut first_archive)?;
        let reader = Box::pin(BufReader::new(Cursor::new(first_archive)));
        let mut importer = ArchiveImporter::load(reader, &key).await?;
        insert_importer(&mut importer, &bo.context).await?;

        let imported = bo
            .db()
            .get_group_message(&message_id)?
            .expect("imported message");
        assert_eq!(
            imported.decrypted_message_bytes,
            source_message.decrypted_message_bytes
        );
        assert_eq!(imported.expire_at_ns, source_message.expire_at_ns);
        assert!(!imported.expiry_unknown);
        let permanent_copy = bo
            .db()
            .get_group_message(&permanent_id)?
            .expect("permanent copy");
        assert_eq!(
            (permanent_copy.expire_at_ns, permanent_copy.expiry_unknown),
            (None, false)
        );
        assert_eq!(
            bo.db()
                .find_group(&source.group_id)?
                .expect("imported group")
                .membership_state,
            GroupMembershipState::Restored
        );

        source.add_members(&[bo.inbox_id()]).await?;
        bo.sync_welcomes().await?;
        assert!(bo.group(&source.group_id)?.is_active()?);

        // An older archive lacks expiry_known, even if it carries a stray deadline.
        let unknown_id = vec![0x5b; 32];
        let mut old =
            xmtp_proto::xmtp::device_sync::message_backup::GroupMessageSave::from(permanent);
        old.id = unknown_id.clone();
        old.expiry_known = None;
        old.expires_at_ns = source_message.expire_at_ns;
        let mut old_elements = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
            element: Some(Element::GroupMessage(old)),
        })]);
        insert_elements(&mut old_elements, &bo.context).await?;
        let unknown = bo
            .db()
            .get_group_message(&unknown_id)?
            .expect("old archive row");
        assert_eq!((unknown.expire_at_ns, unknown.expiry_unknown), (None, true));

        let mut excluded = options.clone();
        excluded.exclude_disappearing_messages = true;
        let mut second_archive = Vec::new();
        exporter::export(excluded, bo.db(), &key, &mut second_archive)?;
        let reader = Box::pin(BufReader::new(Cursor::new(second_archive)));
        let messages: Vec<_> = ArchiveImporter::load(reader, &key)
            .await?
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<_, _>>()?;
        let saved: Vec<_> = messages
            .into_iter()
            .filter_map(|element| match element.element {
                Some(Element::GroupMessage(saved)) => Some(saved),
                _ => None,
            })
            .collect();
        assert_eq!(
            saved.len(),
            1,
            "only known non-disappearing history is exported"
        );
        assert_eq!(saved[0].id, permanent_id);
        assert_eq!(
            (saved[0].expiry_known, saved[0].expires_at_ns),
            (Some(true), None)
        );

        let mut default_export = Vec::new();
        exporter::export(options, bo.db(), &key, &mut default_export)?;
        let reader = Box::pin(BufReader::new(Cursor::new(default_export)));
        let exported = ArchiveImporter::load(reader, &key)
            .await?
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let saved = exported
            .into_iter()
            .find_map(|element| match element.element {
                Some(Element::GroupMessage(saved)) if saved.id == unknown_id => Some(saved),
                _ => None,
            })
            .expect("default export retains unknown history");
        assert_eq!(
            (saved.expiry_known, saved.expires_at_ns),
            (Some(false), None)
        );
    }

    /// A DM restored before a failing element is a completed element. Its
    /// archived activity must survive the failed import, even when no
    /// retained message carries that timestamp.
    // verifies: ARCH-027
    #[xmtp_common::test(unwrap_try = true)]
    async fn failed_import_keeps_archived_activity_of_restored_prefix() {
        use xmtp_mls_common::group_metadata::DmMembers;
        use xmtp_proto::xmtp::device_sync::{
            group_backup::GroupSave,
            message_backup::{GroupMessageKindSave, GroupMessageSave},
        };

        tester!(caro, disable_workers);
        let pair = DmMembers {
            member_one_inbox_id: hex::encode([0x41; 32]),
            member_two_inbox_id: hex::encode([0x42; 32]),
        };
        let group_id = vec![0x65; 16];
        let archived_activity = 1_234_567;
        let group = GroupSave {
            id: group_id.clone(),
            conversation_type: 2,
            dm_id: Some(pair.to_string()),
            last_message_ns: Some(archived_activity),
            ..Default::default()
        };
        // A recognized message element with an unspecified kind is malformed.
        let malformed = GroupMessageSave {
            id: vec![0x66; 32],
            group_id: group_id.clone(),
            kind: GroupMessageKindSave::Unspecified as i32,
            sender_inbox_id: pair.member_one_inbox_id.clone(),
            ..Default::default()
        };

        let events = caro.context.events().subscribe(
            xmtp_events::EventFilter::new([xmtp_events::EventKind::ArchiveRestored]),
            Some(4),
        );
        let mut elements = futures::stream::iter([
            Ok::<_, std::io::Error>(BackupElement {
                element: Some(Element::Group(group)),
            }),
            Ok(BackupElement {
                element: Some(Element::GroupMessage(malformed.clone())),
            }),
        ]);
        let error = insert_elements(&mut elements, &caro.context)
            .await
            .expect_err("a malformed element fails the import");
        assert!(
            matches!(
                error,
                DeviceSyncError::ProtoConversion(xmtp_proto::ConversionError::Unspecified(
                    "message_kind"
                ))
            ),
            "unexpected import error: {error:?}"
        );

        let restored = caro
            .db()
            .find_group(&GroupId::try_from(group_id.as_slice())?)??;
        assert_eq!(restored.membership_state, GroupMembershipState::Restored);
        assert_eq!(restored.last_message_ns, Some(archived_activity));
        assert!(caro.db().get_group_message(&malformed.id)?.is_none());
        assert!(matches!(
            events.drain().as_slice(),
            [xmtp_events::EventEnvelope {
                client: Some(ClientEvent::ArchiveRestored(restored)), ..
            }] if !restored.complete
        ));
    }

    // verifies: DMS-001, ARCH-027
    #[xmtp_common::test(unwrap_try = true)]
    async fn malformed_archived_dm_id_fails_before_group_insert() {
        tester!(alix, disable_workers);
        let group_id = vec![0x64; 16];
        let save = xmtp_proto::xmtp::device_sync::group_backup::GroupSave {
            id: group_id.clone(),
            conversation_type: 2,
            dm_id: Some("dm:malformed".into()),
            ..Default::default()
        };
        let mut elements = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
            element: Some(Element::Group(save)),
        })]);
        assert!(insert_elements(&mut elements, &alix.context).await.is_err());
        assert!(
            alix.db()
                .find_group(&GroupId::try_from(group_id.as_slice())?)?
                .is_none()
        );
    }

    // verifies: EVENT-001, EVENT-017
    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_buffer_export_import() {
        use futures::io::BufReader;
        use futures_util::AsyncReadExt;

        tester!(alix);
        tester!(bo);

        let alix_group = alix.create_group(None, None).unwrap();
        alix_group.add_members(&[bo.inbox_id()]).await.unwrap();
        alix_group
            .send_message(b"hello there", SendMessageOpts::default())
            .await
            .unwrap();

        let opts = ArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            exclude_disappearing_messages: false,
        };

        let key = vec![7; 32];

        let file = {
            let mut file = Vec::new();
            exporter::export(opts, alix.db(), &key, &mut file).unwrap();
            file
        };

        let alix2_wallet = generate_local_wallet();
        let alix2 = ClientBuilder::new_test_client(&alix2_wallet).await;
        let events = alix2.context.events().subscribe(
            xmtp_events::EventFilter::new([xmtp_events::EventKind::ArchiveRestored]),
            Some(4),
        );

        // No messages
        let messages: Vec<StoredGroupMessage> = alix2
            .context
            .db()
            .raw_query(|conn| {
                group_messages::table
                    .select(StoredGroupMessage::as_select())
                    .load(conn)
            })
            .unwrap();
        assert_eq!(messages.len(), 0);

        let reader = BufReader::new(Cursor::new(file.clone()));
        let reader = Box::pin(reader);
        let mut importer = ArchiveImporter::load(reader, &key).await.unwrap();
        insert_importer(&mut importer, &alix2.context)
            .await
            .unwrap();
        assert!(matches!(
            events.drain().as_slice(),
            [xmtp_events::EventEnvelope {
                client: Some(xmtp_events::ClientEvent::ArchiveRestored(restored)), ..
            }] if restored.complete
        ));

        let reader = Box::pin(BufReader::new(Cursor::new(file)));
        let mut second = ArchiveImporter::load(reader, &key).await.unwrap();
        insert_importer(&mut second, &alix2.context).await.unwrap();
        assert!(events.drain().is_empty());

        // One message.
        let messages: Vec<StoredGroupMessage> = alix2
            .context
            .db()
            .raw_query(|conn| {
                group_messages::table
                    .select(StoredGroupMessage::as_select())
                    .load(conn)
            })
            .unwrap();
        assert_eq!(messages.len(), 1);
    }

    // verifies: JOIN-080
    #[xmtp_common::test(unwrap_try = true)]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_file_backup() {
        use crate::{groups::send_message_opts::SendMessageOpts, tester};
        use diesel::QueryDsl;
        use xmtp_db::group::{ConversationType, GroupQueryArgs};

        tester!(alix, sync_worker, triggers);
        tester!(bo);

        let alix_group = alix.create_group(None, None)?;

        // wait for user preference update
        wait_for_min_intents(&alix.context.db(), 2).await?;

        alix_group.add_members(&[bo.inbox_id()]).await?;
        alix_group.update_group_name("My group".to_string()).await?;

        bo.sync_welcomes().await?;
        let bo_group = bo.group(&alix_group.group_id)?;

        // wait for add member intent/commit
        wait_for_min_intents(&alix.context.db(), 1).await?;

        alix_group
            .send_message(b"hello there", SendMessageOpts::default())
            .await?;

        // wait for send message intent/commit publish
        // Wait for Consent state update
        wait_for_min_intents(&alix.context.db(), 4).await?;

        let mut consent_records: Vec<StoredConsentRecord> = alix
            .context
            .db()
            .raw_query(|conn| consent_records::table.load(conn))?;
        assert_eq!(consent_records.len(), 1);
        let old_consent_record = consent_records.pop()?;

        let mut groups: Vec<StoredGroup> = alix
            .context
            .db()
            .raw_query(|conn| groups::table.load(conn))?;
        assert_eq!(groups.len(), 2);
        let old_group = groups.pop()?;

        // The sync group's messages vary with what the outbox owed; only the
        // conversation's messages are archived.
        let old_messages: Vec<StoredGroupMessage> = alix.context.db().raw_query(|conn| {
            group_messages::table
                .select(StoredGroupMessage::as_select())
                .filter(group_messages::group_id.eq(&alix_group.group_id))
                .load(conn)
        })?;
        assert!(!old_messages.is_empty());

        let opts = ArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            exclude_disappearing_messages: false,
        };

        let key = xmtp_common::rand_vec::<32>();
        let path = Path::new("archive.xmtp");
        let _ = tokio::fs::remove_file(path).await;
        exporter::ArchiveExporter::export_to_file(opts, alix.db(), path, &key).await?;

        tester!(alix2, sync_worker);
        alix2.device_sync_client().wait_for_sync_worker_init().await;

        // No consent before
        let consent_records: Vec<StoredConsentRecord> = alix2
            .context
            .db()
            .raw_query(|conn| consent_records::table.load(conn))?;
        assert_eq!(consent_records.len(), 0);

        let mut importer = ArchiveImporter::from_file(path, &key).await?;
        insert_importer(&mut importer, &alix2.context)
            .await
            .unwrap();

        // Consent is there after the import
        let consent_records: Vec<StoredConsentRecord> = alix2
            .context
            .db()
            .raw_query(|conn| consent_records::table.load(conn))?;
        assert_eq!(consent_records.len(), 1);
        // It's the same consent record.
        assert_eq!(
            (consent_records[0].state, consent_records[0].consented_at_ns),
            (old_consent_record.state, old_consent_record.consented_at_ns)
        );

        let groups: Vec<StoredGroup> = alix2.context.db().raw_query(|conn| {
            groups::table
                .filter(groups::conversation_type.ne_all(ConversationType::virtual_types()))
                .load(conn)
        })?;
        assert_eq!(groups.len(), 1);
        // It's the same group
        assert_eq!(groups[0].id, old_group.id);

        let messages: Vec<StoredGroupMessage> = alix2.context.db().raw_query(|conn| {
            group_messages::table
                .select(StoredGroupMessage::as_select())
                .filter(group_messages::group_id.eq(&groups[0].id))
                .load(conn)
        })?;
        // Only the application messages should sync
        assert_eq!(messages.len(), 1);
        for msg in messages {
            let old_msg = old_messages.iter().find(|m| msg.id == m.id)?;
            assert_eq!(old_msg.authority_id, msg.authority_id);
            assert_eq!(old_msg.decrypted_message_bytes, msg.decrypted_message_bytes);
            assert_eq!(old_msg.sent_at_ns, msg.sent_at_ns);
            assert_eq!(old_msg.sender_installation_id, msg.sender_installation_id);
            assert_eq!(old_msg.sender_inbox_id, msg.sender_inbox_id);
            assert_eq!(old_msg.group_id, msg.group_id);
        }

        let alix2_group = alix2.group(&old_group.id)?;
        // Loading all the groups works fine
        let _groups = alix2.find_groups(GroupQueryArgs::default())?;
        // Can fetch the group name no problem
        alix2_group.group_name()?;
        assert!(!alix2_group.is_active()?);

        // Add the new inbox to the groups
        alix.group(&old_group.id)?
            .add_members(&[alix2.inbox_id()])
            .await?;
        alix2.sync_welcomes().await?;

        // The group restores to being fully functional
        let alix2_group = alix2.group(&old_group.id)?;
        assert!(alix2_group.is_active()?);

        // The old messages should be stitched in
        let msgs = alix2_group.find_messages(&MsgQueryArgs::default())?;
        let old_msg_exists = msgs
            .iter()
            .any(|msg| msg.decrypted_message_bytes == b"hello there");
        assert!(old_msg_exists);

        // Bo should see the new message from alix2
        alix2_group
            .send_message(b"this should send", SendMessageOpts::default())
            .await?;
        bo_group.sync().await?;
        let msgs = bo_group.find_messages(&MsgQueryArgs::default())?;
        let new_msg_exists = msgs
            .iter()
            .any(|msg| msg.decrypted_message_bytes == b"this should send");
        assert!(new_msg_exists);

        // cleanup
        let _ = tokio::fs::remove_file(path).await;
    }

    #[xmtp_common::test(unwrap_try = true)]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_legacy_archive_import() {
        use std::path::PathBuf;

        use crate::tester;

        let key = vec![0; 32];
        let path = PathBuf::from("tests/assets/archive-legacy.xmtp");
        let mut importer = ArchiveImporter::from_file(path, &key).await?;

        tester!(alix);

        let result = insert_importer(&mut importer, &alix.context).await;
        assert!(result.is_ok());
    }

    /// This archive was generated by the legacy creation build before groups
    /// were born with an AppData dictionary. Import must preserve the saved
    /// metadata while it creates dictionary-native backup stubs.
    // verifies: ARCH-022
    #[xmtp_common::test(unwrap_try = true)]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_import_current_legacy_archive_metadata() {
        use std::path::PathBuf;

        use crate::groups::group_permissions::PolicySet;
        use xmtp_db::group::ConversationType;

        let key = vec![9; 32];
        let path = PathBuf::from("tests/assets/archive-current-legacy-metadata.xmtp");
        let mut preview = ArchiveImporter::from_file(path.clone(), &key).await?;
        let mut archived_dm_id = None;
        while let Some(element) = preview.next().await {
            if let Some(Element::Group(group)) = element?.element {
                archived_dm_id = archived_dm_id.or(group.dm_id);
            }
        }
        let archived_dm_id = archived_dm_id.expect("legacy DM pair");
        let mut importer = ArchiveImporter::from_file(path, &key).await?;
        tester!(alix, disable_workers);
        let pair = crate::groups::parse_canonical_dm_id(Some(&archived_dm_id))?;
        assert_ne!(pair.member_one_inbox_id, alix.inbox_id());
        assert_ne!(pair.member_two_inbox_id, alix.inbox_id());
        insert_importer(&mut importer, &alix.context).await?;

        let groups: Vec<StoredGroup> = alix
            .context
            .db()
            .raw_query(|conn| groups::table.load(conn))?;
        assert_eq!(groups.len(), 2);
        for record in &groups {
            let group = alix.group(&record.id)?;
            group.with_group_snapshot(|mls| {
                assert!(mls.extensions().app_data_dictionary().is_some());
                Ok(())
            })?;
        }

        let restored_group_record = groups
            .iter()
            .find(|group| group.conversation_type == ConversationType::Group)
            .expect("legacy group archive entry");
        assert_eq!(restored_group_record.message_disappear_from_ns, Some(123));
        assert_eq!(restored_group_record.message_disappear_in_ns, Some(456));
        let restored_group = alix.group(&restored_group_record.id)?;
        assert_eq!(restored_group.group_name()?, "legacy archive group name");
        assert_eq!(
            restored_group.group_description()?,
            "legacy archive group description"
        );
        assert_eq!(
            restored_group.group_image_url_square()?,
            "https://example.com/legacy-archive.png"
        );
        assert_eq!(restored_group.app_data()?, "legacy archive app data");
        assert_eq!(restored_group.permissions()?.policies, PolicySet::default());

        let restored_dm_record = groups
            .iter()
            .find(|group| group.conversation_type == ConversationType::Dm)
            .expect("legacy DM archive entry");
        assert_eq!(
            restored_dm_record.dm_id.as_deref(),
            Some(archived_dm_id.as_str())
        );
        assert_eq!(restored_dm_record.message_disappear_from_ns, Some(123));
        assert_eq!(restored_dm_record.message_disappear_in_ns, Some(456));
        let restored_dm = alix.group(&restored_dm_record.id)?;
        assert_eq!(restored_dm.group_name()?, "legacy archive dm name");
        assert_eq!(
            restored_dm.group_description()?,
            "legacy archive dm description"
        );
        assert_eq!(
            restored_dm.group_image_url_square()?,
            "https://example.com/legacy-dm-archive.png"
        );
        assert_eq!(restored_dm.app_data()?, "legacy archive dm app data");
    }

    /// Dictionary metadata must survive archive export and import.
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_archive_includes_dictionary_groups() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);

        let alix_group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        alix_group.send_message(b"hi", Default::default()).await?;

        // Update metadata on the dictionary-native group so the exported
        // values come from the AppData dictionary.

        alix_group
            .update_group_name("post-migration name".to_string())
            .await?;
        alix_group
            .update_group_description("post-migration description".to_string())
            .await?;
        alix_group
            .update_group_image_url_square("https://example.com/post-migration.png".to_string())
            .await?;

        // A second group must restore alongside the first group.
        let second_group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        second_group
            .update_group_name("second group name".to_string())
            .await?;

        let key = vec![7; 32];
        let opts = ArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            exclude_disappearing_messages: false,
        };
        let export = {
            let mut file = vec![];
            exporter::export(opts, alix.db(), &key, &mut file)?;
            file
        };

        // Fresh installation of the same inbox restores from the
        // archive only (workers disabled, no welcome sync).
        tester!(alix2, from: alix);
        let reader = Box::pin(BufReader::new(Cursor::new(export)));
        let mut importer = ArchiveImporter::load(reader, &key).await?;
        insert_importer(&mut importer, &alix2.context).await?;

        let restored = alix2.db().find_group(&alix_group.group_id)?;
        assert!(
            restored.is_some(),
            "dictionary group missing from restored archive"
        );

        // Presence is not enough: the metadata written before export
        // must round-trip through the archive, or per-field loss in
        // the exporter's dict read would go unnoticed.
        let restored_group = alix2.group(&alix_group.group_id)?;
        assert_eq!(restored_group.group_name()?, "post-migration name");
        assert_eq!(
            restored_group.group_description()?,
            "post-migration description"
        );
        assert_eq!(
            restored_group.group_image_url_square()?,
            "https://example.com/post-migration.png"
        );

        let restored_second = alix2.group(&second_group.group_id)?;
        assert_eq!(restored_second.group_name()?, "second group name");
    }
}

#[cfg(test)]
mod pre8_tests;

#[cfg(test)]
mod restored_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod migration_tests;
