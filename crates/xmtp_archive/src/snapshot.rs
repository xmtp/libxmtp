//! The export snapshot: every selected element, read in one SQLite read
//! transaction and measured at one export time.
//!
//! [`read`] runs `BEGIN` (deferred, read-only) on one connection, runs the
//! selection queries below, loads each group's MLS state through that same
//! connection, and commits before it returns. Rows stream to the caller while
//! the transaction is open: groups a page at a time, messages and consent
//! through row cursors, so memory stays bounded by a page of groups however
//! large the history is. The transaction lasts as long as the export and
//! never spans an await. Under WAL, writers proceed concurrently; with a
//! single shared connection, other queries wait for the export.

use crate::{
    ArchiveError, UnreadableGroup,
    archive_options::{ArchiveOptions, BackupElementSelection},
};
use openmls::group::MlsGroup;
use prost::Message;
use std::collections::HashMap;
use xmtp_common::time::now_ns;
use xmtp_db::{
    ConnectionExt, TransactionalKeyStore, XmtpMlsStorageProvider,
    consent_record::StoredConsentRecord,
    diesel::{Connection, SqliteConnection, connection::DefaultLoadingMode, prelude::*, sql_query},
    group::{ConversationType, GroupMembershipState, StoredGroup},
    group_message::{GroupMessageKind, StoredGroupMessage},
    schema::{consent_records, group_messages, groups, restored_group_metadata},
};
use xmtp_mls_common::{
    group_metadata::extract_group_metadata,
    group_mutable_metadata::{GroupMutableMetadata, merge_dict_into_mutable_metadata},
};
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::device_sync::{
    BackupElementSelection as BackupElementSelectionProto, BackupMetadataSave,
    backup_element::Element,
    group_backup::{
        ConversationTypeSave, GroupMembershipStateSave, GroupSave, ImmutableMetadataSave,
        MutableMetadataSave,
    },
};

/// Groups loaded per page. Each group's MLS state is loaded through the
/// connection, so groups are paged by key rather than read through a cursor.
const GROUP_PAGE: i64 = 100;

/// Passes `emit` the archive metadata, then every element `opts` selects, as
/// stored when the transaction began: all eligible groups, then their
/// messages, then consent. The export time is measured once the snapshot is
/// taken, so nothing emitted postdates it. Fails, rather than omitting it, on
/// any eligible group whose MLS state or immutable metadata cannot be read.
/// `emit` runs inside the transaction and must not use the database. Returns
/// the metadata.
pub(crate) fn read(
    db: &impl ConnectionExt,
    opts: &ArchiveOptions,
    mut emit: impl FnMut(Element) -> Result<(), ArchiveError>,
) -> Result<BackupMetadataSave, ArchiveError> {
    db.raw_query(|conn| Ok(conn.transaction(|conn| read_in_transaction(conn, opts, &mut emit))))?
}

fn read_in_transaction(
    conn: &mut SqliteConnection,
    opts: &ArchiveOptions,
    emit: &mut impl FnMut(Element) -> Result<(), ArchiveError>,
) -> Result<BackupMetadataSave, ArchiveError> {
    // A deferred transaction takes its snapshot at its first read.
    sql_query("SELECT 1 FROM sqlite_master LIMIT 1").execute(conn)?;
    let exported_at_ns = now_ns();
    let metadata = BackupMetadataSave {
        elements: opts
            .elements
            .iter()
            .map(|&e| BackupElementSelectionProto::from(e) as i32)
            .collect(),
        exported_at_ns,
        start_ns: opts.start_ns,
        end_ns: opts.end_ns,
    };
    emit(Element::Metadata(metadata.clone()))?;
    let selects = |s| opts.elements.contains(&s);
    if selects(BackupElementSelection::Messages) {
        let mut after = None;
        loop {
            let mut page = groups::table
                .filter(groups::conversation_type.ne_all(ConversationType::virtual_types()))
                .order(groups::id)
                .limit(GROUP_PAGE)
                .into_boxed();
            if let Some(id) = after {
                page = page.filter(groups::id.gt(id));
            }
            let page = page.load::<StoredGroup>(conn)?;
            let Some(last) = page.last() else { break };
            after = Some(last.id);
            let mut histories = restored_histories(conn, &page)?;
            let store = conn.key_store();
            for group in page {
                let save = match histories.remove(&group.id) {
                    Some(history) => restored_group_save(group, history),
                    None => group_save(&store, group)?,
                };
                emit(Element::Group(save))?;
            }
        }
        let mut messages = group_messages::table
            .inner_join(groups::table)
            .filter(groups::conversation_type.ne_all(ConversationType::virtual_types()))
            .filter(group_messages::kind.eq(GroupMessageKind::Application))
            .select(StoredGroupMessage::as_select())
            .order(group_messages::id)
            .into_boxed();
        if let Some(start_ns) = opts.start_ns {
            messages = messages.filter(group_messages::sent_at_ns.gt(start_ns));
        }
        if let Some(end_ns) = opts.end_ns {
            messages = messages.filter(group_messages::sent_at_ns.le(end_ns));
        }
        messages = if opts.exclude_disappearing_messages {
            messages.filter(group_messages::expire_at_ns.is_null())
        } else {
            messages.filter(
                group_messages::expire_at_ns
                    .is_null()
                    .or(group_messages::expire_at_ns.gt(exported_at_ns)),
            )
        };
        for message in messages.load_iter::<StoredGroupMessage, DefaultLoadingMode>(conn)? {
            emit(Element::GroupMessage(message?.into()))?;
        }
    }
    if selects(BackupElementSelection::Consent) {
        let consent = consent_records::table
            .order((consent_records::entity_type, consent_records::entity))
            .load_iter::<StoredConsentRecord, DefaultLoadingMode>(conn)?;
        for record in consent {
            emit(Element::Consent(record?.into()))?;
        }
    }
    Ok(metadata)
}

/// The archived records of the `Restored` groups in `page`, keyed by id. A
/// Restored conversation re-exports its archived record, with the presence of
/// its metadata message unchanged, never the metadata of its placeholder MLS
/// state.
// implements: ARCH-025
fn restored_histories(
    conn: &mut SqliteConnection,
    page: &[StoredGroup],
) -> Result<HashMap<GroupId, GroupSave>, ArchiveError> {
    let ids: Vec<GroupId> = page
        .iter()
        .filter(|group| group.membership_state == GroupMembershipState::Restored)
        .map(|group| group.id)
        .collect();
    let mut histories = HashMap::with_capacity(ids.len());
    if ids.is_empty() {
        return Ok(histories);
    }
    let rows = restored_group_metadata::table
        .filter(restored_group_metadata::group_id.eq_any(&ids))
        .select((
            restored_group_metadata::group_id,
            restored_group_metadata::group_save,
        ))
        .load::<(GroupId, Vec<u8>)>(conn)?;
    for (group_id, bytes) in rows {
        histories.insert(group_id, GroupSave::decode(bytes.as_slice())?);
    }
    Ok(histories)
}

/// The archived record of a Restored group. Only the physical id, the
/// membership state, and the merged activity come from the current row.
fn restored_group_save(group: StoredGroup, history: GroupSave) -> GroupSave {
    let membership_state: GroupMembershipStateSave = group.membership_state.into();
    GroupSave {
        id: group.id.to_vec(),
        membership_state: membership_state as i32,
        last_message_ns: group.last_message_ns,
        ..history
    }
}

/// The group element for `group`, with metadata read from its MLS state.
fn group_save(
    store: &impl XmtpMlsStorageProvider,
    group: StoredGroup,
) -> Result<GroupSave, ArchiveError> {
    let group_id = group.id;
    let unreadable = |source: UnreadableGroup| ArchiveError::UnreadableGroup { group_id, source };
    let mls_group = MlsGroup::load(store, &group.id.to_openmls())
        .map_err(|e| unreadable(e.into()))?
        .ok_or_else(|| unreadable(UnreadableGroup::MissingState))?;
    let extensions = mls_group.extensions();
    let immutable = extract_group_metadata(extensions).map_err(|e| unreadable(e.into()))?;
    let mut mutable = GroupMutableMetadata::new(Default::default(), Vec::new(), Vec::new());
    merge_dict_into_mutable_metadata(&mut mutable, extensions).map_err(|e| unreadable(e.into()))?;
    let membership_state: GroupMembershipStateSave = group.membership_state.into();
    let conversation_type: ConversationTypeSave = group.conversation_type.into();
    Ok(GroupSave {
        id: group.id.to_vec(),
        created_at_ns: group.created_at_ns,
        membership_state: membership_state as i32,
        installations_last_checked: group.installations_last_checked,
        added_by_inbox_id: group.added_by_inbox_id,
        welcome_id: group.sequence_id,
        rotated_at_ns: group.rotated_at_ns,
        conversation_type: conversation_type as i32,
        dm_id: group.dm_id,
        last_message_ns: group.last_message_ns,
        message_disappear_from_ns: group.message_disappear_from_ns,
        message_disappear_in_ns: group.message_disappear_in_ns,
        paused_for_version: group.paused_for_version,
        metadata: Some(ImmutableMetadataSave {
            creator_inbox_id: immutable.creator_inbox_id,
        }),
        mutable_metadata: Some(MutableMetadataSave {
            attributes: mutable.attributes,
            admin_list: mutable.admin_list,
            super_admin_list: mutable.super_admin_list,
        }),
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        ArchiveError, ArchiveImporter, BACKUP_VERSION, NONCE_SIZE,
        archive_options::{ArchiveOptions, BackupElementSelection},
        exporter,
        util::GenericArrayExt,
    };
    use aes_gcm::aead::Aead;
    use futures::{
        AsyncReadExt, StreamExt,
        io::{BufReader, Cursor},
    };
    use prost::Message;
    #[allow(deprecated)]
    use sha2::digest::generic_array::GenericArray;
    use xmtp_db::{
        Store, TestDb, XmtpTestDb,
        consent_record::{ConsentState, ConsentType, StoredConsentRecord},
        group::{ConversationType, GroupMembershipState, StoredGroup},
    };
    use xmtp_proto::xmtp::device_sync::BackupElement;
    use xmtp_proto::{types::GroupId, xmtp::device_sync::backup_element::Element};

    const KEY: [u8; 32] = [7; 32];

    fn group(id: [u8; 16], conversation_type: ConversationType) -> StoredGroup {
        StoredGroup::builder()
            .id(GroupId::from(id))
            .created_at_ns(1)
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id("adder")
            .conversation_type(conversation_type)
            .build()
            .unwrap()
    }

    fn options(elements: &[BackupElementSelection]) -> ArchiveOptions {
        ArchiveOptions {
            elements: elements.to_vec(),
            start_ns: Some(1_000),
            end_ns: Some(2_000),
            exclude_disappearing_messages: false,
        }
    }

    /// Every element of `archive` after the metadata frame.
    async fn elements(archive: Vec<u8>) -> Vec<Element> {
        let reader = Box::pin(BufReader::new(Cursor::new(archive)));
        ArchiveImporter::load(reader, &KEY)
            .await
            .unwrap()
            .map(|e| e.unwrap().element.unwrap())
            .collect()
            .await
    }

    /// A sink that accepts one to three bytes per write, interrupts every
    /// other write, and fails its first flush when asked.
    struct ShortSink {
        bytes: Vec<u8>,
        writes: usize,
        flushes: usize,
        fail_first_flush: bool,
    }

    impl std::io::Write for ShortSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.writes += 1;
            if self.writes.is_multiple_of(2) {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            let amount = buf.len().min(1 + self.writes % 3);
            self.bytes.extend_from_slice(&buf[..amount]);
            Ok(amount)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            if self.fail_first_flush && self.flushes == 1 {
                return Err(std::io::Error::other("flush failed"));
            }
            Ok(())
        }
    }

    /// The archive is the version, the starting nonce, then one zstd stream
    /// of length-prefixed AES-GCM frames under counter nonces: the metadata,
    /// then each element. Short and interrupted writes change none of those
    /// bytes, and a sink that fails a flush fails the export.
    // verifies: ARCH-001
    #[xmtp_common::test(unwrap_try = true)]
    async fn short_and_interrupted_writes_preserve_exact_frames() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        let mut records = Vec::new();
        for entity in ["alice", "bob", "carol"] {
            let record = StoredConsentRecord::new(
                ConsentType::InboxId,
                ConsentState::Allowed,
                entity.into(),
            );
            record.store(&db)?;
            records.push(record);
        }
        let consent = options(&[BackupElementSelection::Consent]);

        let mut sink = ShortSink {
            bytes: Vec::new(),
            writes: 0,
            flushes: 0,
            fail_first_flush: true,
        };
        assert!(
            exporter::export(consent.clone(), &db, &KEY, &mut sink).is_err(),
            "a failed flush did not fail the export"
        );

        let mut sink = ShortSink {
            bytes: Vec::new(),
            writes: 0,
            flushes: 0,
            fail_first_flush: false,
        };
        let metadata = exporter::export(consent, &db, &KEY, &mut sink)?;
        assert!(sink.writes > 3, "the sink saw no short writes");
        let archive = sink.bytes;

        // The header is in the clear: the version, then the starting nonce.
        assert_eq!(&archive[..2], &BACKUP_VERSION.to_le_bytes());
        #[allow(deprecated)]
        let mut nonce = GenericArray::clone_from_slice(&archive[2..2 + NONCE_SIZE]);
        let cipher = crate::cipher(&KEY)?;
        let mut expected = Vec::new();
        let elements = std::iter::once(Element::Metadata(metadata)).chain(
            records
                .into_iter()
                .map(|record| Element::Consent(record.into())),
        );
        for element in elements {
            let plaintext = BackupElement {
                element: Some(element),
            }
            .encode_to_vec();
            let ciphertext = cipher.encrypt(&nonce, &*plaintext)?;
            nonce.increment();
            expected.extend_from_slice(&(ciphertext.len() as u32).to_le_bytes());
            expected.extend_from_slice(&ciphertext);
        }
        let mut frames = Vec::new();
        async_compression::futures::bufread::ZstdDecoder::new(Cursor::new(
            &archive[2 + NONCE_SIZE..],
        ))
        .read_to_end(&mut frames)
        .await?;
        assert!(
            frames == expected,
            "the frames differ from the counter-nonce sequence"
        );
    }

    /// An export reads every eligible conversation: an eligible group it
    /// cannot read fails the export, and leaves no archive file, even when the
    /// group was created outside the window, has no messages, and sorts after
    /// more than a page of excluded internal conversations. An explicit empty
    /// selection exports nothing. Restore and concurrent-write coverage for
    /// readable conversations lives in xmtp_mls.
    // verifies: ARCH-007, ARCH-017
    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_snapshot_is_complete() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        for i in 0..150u8 {
            let kind = [ConversationType::Sync, ConversationType::Oneshot][usize::from(i % 2)];
            let mut id = [0; 16];
            id[15] = i;
            group(id, kind).store(&db)?;
        }
        let unreadable = group([0xff; 16], ConversationType::Group);
        unreadable.store(&db)?;
        let messages = options(&[BackupElementSelection::Messages]);

        let failure = exporter::export(messages.clone(), &db, &KEY, Vec::new());
        assert!(
            matches!(
                failure,
                Err(ArchiveError::UnreadableGroup { group_id, .. }) if group_id == unreadable.id
            ),
            "export skipped an unreadable eligible group"
        );
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = xmtp_common::tmp_path();
            let failure = exporter::ArchiveExporter::export_to_file(
                messages.clone(),
                db.clone(),
                &path,
                &KEY,
            )
            .await;
            assert!(failure.is_err());
            assert!(
                !std::path::Path::new(&path).exists(),
                "failed export left a file"
            );
        }

        #[cfg(target_arch = "wasm32")]
        {
            use futures::AsyncReadExt;
            let mut read = Vec::new();
            let failure = exporter::ArchiveExporter::new(messages, &db, &KEY)
                .read_to_end(&mut read)
                .await;
            assert!(
                failure.is_err() && read.is_empty(),
                "stream served a failed export"
            );
        }
        let mut archive = Vec::new();
        let metadata = exporter::export(options(&[]), &db, &KEY, &mut archive)?;
        assert!(metadata.elements.is_empty());
        assert_eq!(elements(archive).await, vec![]);
    }

    /// Writes `on_first` into the database when the export first writes, i.e.
    /// the header, before the snapshot's transaction opens.
    struct WriteBeforeSnapshot<F: FnMut()>(Vec<u8>, Option<F>);

    impl<F: FnMut()> std::io::Write for WriteBeforeSnapshot<F> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Some(mut f) = self.1.take() {
                f();
            }
            self.0.write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// The export time is measured once the snapshot is taken, so nothing in
    /// the archive postdates it: a record committed while a slow sink takes
    /// the header is exported at a time after that record, never before it.
    // verifies: ARCH-017
    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_export_time_follows_its_contents() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        let consent = options(&[BackupElementSelection::Consent]);
        let mut sink = WriteBeforeSnapshot(
            Vec::new(),
            Some(|| {
                StoredConsentRecord::new(
                    ConsentType::InboxId,
                    ConsentState::Allowed,
                    "carol".into(),
                )
                .store(&db)
                .unwrap();
            }),
        );

        let metadata = exporter::export(consent, &db, &KEY, &mut sink)?;
        let [Element::Consent(record)] = &elements(sink.0).await[..] else {
            panic!("the record committed before the snapshot is missing");
        };
        assert!(
            record.consented_at_ns <= metadata.exported_at_ns,
            "archived a record from after its export time"
        );
    }

    /// A key that is not 32 bytes, shorter or longer, is rejected before any
    /// archive byte is written or read: the sink stays empty, an existing
    /// destination file is left untouched, and import fails before it reads
    /// the header. A longer key is never truncated, so two keys sharing a
    /// 32-byte prefix can never open each other's archives.
    // verifies: ARCH-012
    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_rejects_a_wrong_length_key_before_any_byte() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        let consent = options(&[BackupElementSelection::Consent]);
        let long = [7; 33];
        for key in [&KEY[..31], &long[..]] {
            let wrong = |r: Result<_, ArchiveError>| matches!(r, Err(ArchiveError::InvalidKeyLength(n)) if n == key.len());
            let mut sink = Vec::new();
            let failure = exporter::export(consent.clone(), &db, key, &mut sink);
            assert!(
                wrong(failure.map(drop)),
                "export accepted a {}-byte key",
                key.len()
            );
            assert!(sink.is_empty(), "export wrote with an invalid key");
            #[cfg(not(target_arch = "wasm32"))]
            {
                let path = xmtp_common::tmp_path();
                std::fs::write(&path, b"prior")?;
                let failure = exporter::ArchiveExporter::export_to_file(
                    consent.clone(),
                    db.clone(),
                    &path,
                    key,
                )
                .await;
                assert!(
                    wrong(failure.map(drop)),
                    "file export accepted a {}-byte key",
                    key.len()
                );
                assert_eq!(std::fs::read(&path)?, b"prior", "export touched the file");
            }
            let empty = Box::pin(BufReader::new(Cursor::new(Vec::new())));
            let failure = ArchiveImporter::load(empty, key).await;
            assert!(
                wrong(failure.map(drop)),
                "import accepted a {}-byte key",
                key.len()
            );
        }
    }

    /// Only transient archive failures are retryable: interrupted or
    /// timed-out I/O, or a group whose MLS state read hit a busy or dropped
    /// connection, may succeed on a second attempt. A truncated or corrupt
    /// archive, a failed write, a wrong key, or a group with no state fails
    /// the same way every time, so retrying it only delays the error.
    #[xmtp_common::test]
    fn archive_errors_are_retryable_only_when_transient() {
        use crate::UnreadableGroup;
        use std::io::{Error, ErrorKind::*};
        use xmtp_common::RetryableError;
        use xmtp_db::{ConnectionError, sql_key_store::SqlKeyStoreError};
        let unreadable = |source| ArchiveError::UnreadableGroup {
            group_id: GroupId::from([1; 16]),
            source,
        };
        for kind in [Interrupted, WouldBlock, TimedOut] {
            assert!(
                ArchiveError::IO(kind.into()).is_retryable(),
                "{kind} is terminal"
            );
        }
        let dropped = SqlKeyStoreError::Connection(ConnectionError::DisconnectInTransaction);
        assert!(unreadable(UnreadableGroup::State(dropped)).is_retryable());
        for terminal in [
            unreadable(UnreadableGroup::MissingState),
            unreadable(UnreadableGroup::State(SqlKeyStoreError::NotFound)),
            ArchiveError::IO(UnexpectedEof.into()),
            ArchiveError::IO(InvalidData.into()),
            ArchiveError::IO(Error::other("archive export cancelled")),
            ArchiveError::InvalidKeyLength(31),
            ArchiveError::MissingMetadata,
            ArchiveError::UnsupportedVersion(u16::MAX),
            ArchiveError::InvalidFrame("cut"),
        ] {
            assert!(!terminal.is_retryable(), "{terminal} is retryable");
        }
    }

    /// A file export whose caller has gone stops at its next write, rather
    /// than finishing an archive nobody awaits. A failed or cancelled export
    /// leaves the archive already at the destination intact and no temporary
    /// file behind: exporting over a good backup must never destroy it. Every
    /// archive, new or replacing one, is owner-only: it never takes the
    /// permissions of whatever the destination held, which anyone able to
    /// write the directory could have planted.
    /// `export_to_file` cancels this token when its future is dropped.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_file_export_stops_when_cancelled() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        let consent = options(&[BackupElementSelection::Consent]);
        let dir = std::env::temp_dir().join(xmtp_common::rand_hexstring());
        std::fs::create_dir(&dir)?;
        let path = dir.join("archive");
        let cancel = tokio_util::sync::CancellationToken::new();

        exporter::write_file(consent.clone(), &db, &path, &KEY, &cancel)?;
        let first = std::fs::read(&path)?;
        #[cfg(unix)]
        let mode = |path: &std::path::Path| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(mode(&path), 0o600, "a new archive is readable by others");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
        }
        exporter::write_file(consent.clone(), &db, &path, &KEY, &cancel)?;
        let prior = std::fs::read(&path)?;
        assert_ne!(
            prior, first,
            "an export did not replace the existing archive"
        );
        #[cfg(unix)]
        assert_eq!(
            mode(&path),
            0o600,
            "an archive took the permissions of the file it replaced"
        );
        cancel.cancel();
        let failure = exporter::write_file(consent, &db, &path, &KEY, &cancel);
        assert!(failure.is_err(), "a cancelled export ran to completion");
        assert_eq!(
            std::fs::read(&path)?,
            prior,
            "a failed export replaced the archive"
        );
        assert_eq!(
            std::fs::read_dir(&dir)?.count(),
            1,
            "a failed export left a temporary file"
        );
        std::fs::remove_dir_all(&dir)?;
    }

    /// Holds the export's first query until the test releases it, so the
    /// test can act while the export is in flight. Dropping it, when the
    /// export finishes with the database, closes `finished`.
    #[cfg(not(target_arch = "wasm32"))]
    struct Gated<C> {
        db: C,
        entered: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
        release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
        _finished: tokio::sync::oneshot::Sender<()>,
    }

    #[cfg(not(target_arch = "wasm32"))]
    impl<C: xmtp_db::ConnectionExt> xmtp_db::ConnectionExt for Gated<C> {
        fn raw_query<T, F>(&self, fun: F) -> Result<T, xmtp_db::ConnectionError>
        where
            F: FnOnce(
                &mut xmtp_db::diesel::SqliteConnection,
            ) -> Result<T, xmtp_db::diesel::result::Error>,
        {
            if let Some(entered) = self.entered.lock().unwrap().take() {
                entered.send(()).unwrap();
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("the gated export held the test's thread");
            }
            self.db.raw_query(fun)
        }

        fn disconnect(&self) -> Result<(), xmtp_db::ConnectionError> {
            self.db.disconnect()
        }

        fn reconnect(&self) -> Result<(), xmtp_db::ConnectionError> {
            self.db.reconnect()
        }
    }

    /// `export_to_bytes` runs on the blocking pool, so a long export never
    /// holds an async worker. On a single-threaded runtime the test keeps
    /// running while the export is blocked mid-snapshot; run inline, the
    /// export would hold the only thread until its gate timed out. The bytes
    /// it returns are a complete archive.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test]
    async fn byte_export_leaves_the_async_worker_free() {
        let store = TestDb::create_ephemeral_store().await;
        let (entered, in_flight) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let db = Gated {
            db: store.db(),
            entered: Some(entered).into(),
            release: released.into(),
            _finished: tokio::sync::oneshot::channel().0,
        };
        let consent = options(&[BackupElementSelection::Consent]);
        let export = tokio::spawn(exporter::ArchiveExporter::export_to_bytes(
            consent, db, &KEY,
        ));

        in_flight.await.unwrap();
        tokio::task::yield_now().await;
        release.send(()).unwrap();
        let archive = export.await.expect("the export ran on the async worker");
        assert!(elements(archive.unwrap()).await.is_empty());
    }

    /// Dropping `export_to_file`'s future while its export is in flight
    /// cancels the export: it stops rather than finishing an archive nobody
    /// awaits, removes its temporary file, and leaves the archive already at
    /// the destination unchanged.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn dropping_a_file_export_cancels_it() {
        let store = TestDb::create_ephemeral_store().await;
        let dir = std::env::temp_dir().join(xmtp_common::rand_hexstring());
        std::fs::create_dir(&dir)?;
        let path = dir.join("archive");
        std::fs::write(&path, b"prior")?;
        let (entered, in_flight) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let (finished, done) = tokio::sync::oneshot::channel::<()>();
        let db = Gated {
            db: store.db(),
            entered: Some(entered).into(),
            release: released.into(),
            _finished: finished,
        };
        let consent = options(&[BackupElementSelection::Consent]);
        let export = tokio::spawn(exporter::ArchiveExporter::export_to_file(
            consent,
            db,
            path.clone(),
            &KEY,
        ));

        in_flight.await?;
        export.abort();
        assert!(export.await.unwrap_err().is_cancelled());
        release.send(())?;
        let _ = done.await;
        // The export has released the database; its cleanup follows at once.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::fs::read_dir(&dir)?.count() > 1 && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        assert_eq!(
            std::fs::read(&path)?,
            b"prior",
            "a dropped export replaced the archive"
        );
        assert_eq!(
            std::fs::read_dir(&dir)?.count(),
            1,
            "a dropped export left a temporary file"
        );
        std::fs::remove_dir_all(&dir)?;
    }

    /// An export over a symlink replaces the link with an owner-only archive
    /// rather than taking the permissions of the link's target, so whoever
    /// can plant a link cannot widen who reads the archive.
    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_over_a_symlink_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let store = TestDb::create_ephemeral_store().await;
        let dir = std::env::temp_dir().join(xmtp_common::rand_hexstring());
        std::fs::create_dir(&dir)?;
        let (target, path) = (dir.join("target"), dir.join("archive"));
        std::fs::write(&target, b"target")?;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644))?;
        std::os::unix::fs::symlink(&target, &path)?;

        let consent = options(&[BackupElementSelection::Consent]);
        exporter::ArchiveExporter::export_to_file(consent, store.db(), &path, &KEY).await?;
        let archive = std::fs::symlink_metadata(&path)?;
        assert!(
            archive.file_type().is_file(),
            "the export wrote through the link"
        );
        assert_eq!(
            archive.permissions().mode() & 0o777,
            0o600,
            "the archive took the link target's permissions"
        );
        assert_eq!(std::fs::read(&target)?, b"target");
        std::fs::remove_dir_all(&dir)?;
    }
}
