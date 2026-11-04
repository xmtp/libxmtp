//! Archive tests: export tests.

use super::*;

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
    use xmtp_mls_group_state::app_data::component_id::ComponentId;

    tester!(alix, disable_workers);
    let conversation = alix.create_group(None, None)?;
    let provider = alix.context.mls_provider();
    let signer = &alix.context.identity().installation_keys;
    let mut group = MlsGroup::load(
        alix.context.mls_storage(),
        &conversation.group_id.to_openmls(),
    )??;
    let malformed = vec![0xff; 3];
    let mut stage = xmtp_mls_engine::groups::test_support::commit_without_group_info(
        &mut group,
        provider.storage(),
        |builder| {
            builder.add_proposal(Proposal::AppDataUpdate(Box::new(
                AppDataUpdateProposal::update(ComponentId::ADMIN_LIST.as_u16(), malformed.clone()),
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
    use xmtp_mls_group_state::app_data::{component_id::ComponentId, group_root::read_group_root};

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
    let mut stage = xmtp_mls_engine::groups::test_support::commit_without_group_info(
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
    for (secret, encoded) in xmtp_mls::utils::test::group_secret_encodings(&root) {
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
