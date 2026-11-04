//! Upload tests: create.

use super::*;

// verifies: ATCH-046, ATCH-063
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_create_rolls_back_both_publication_points() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    for after_staged in [false, true] {
        let pause = Arc::new(CreatePublishPause {
            after_staged,
            path: Mutex::new(None),
            staged_path: Mutex::new(None),
            entered: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        });
        alix.client
            .context
            .attachment_runtime()
            .set_create_publish_pause(Some(pause.clone()));
        let client = alix.client.clone();
        let create = tokio::spawn(async move { client.attachments().create(bytes()).await });
        tokio::time::timeout(Duration::from_secs(3), pause.entered.notified()).await?;
        let path = pause.path.lock().clone().expect("plaintext path");
        let staged = pause.staged_path.lock().clone().expect("staged path");
        create.abort();
        let _ = create.await;
        tokio::time::timeout(
            Duration::from_secs(3),
            alix.client
                .context
                .attachment_runtime()
                .reconcile_again(&alix.client.context),
        )
        .await??;
        let store = alix.client.context.attachment_runtime().local_store()?;
        assert!(!store.exists(&path).await?);
        assert!(!store.exists(&staged).await?);
        assert!(
            alix.client
                .context
                .db()
                .get_local_attachment(&path)?
                .is_none()
        );
        let digest = staged.strip_prefix(".staged/").expect("staged digest");
        assert!(
            alix.client
                .context
                .db()
                .get_pending_attachment(digest)?
                .is_none()
        );
        assert!(store.list_files().await?.is_empty());
    }
}

// verifies: ATCH-030, ATCH-033
#[xmtp_common::test(unwrap_try = true)]
async fn create_preconditions() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: |_configuration: &mut ServerConfiguration| {}, disable_workers);
    let error = alix
        .client
        .attachments()
        .create(bytes())
        .await
        .err()
        .expect("creation must fail");
    assert_eq!(error.cause, Cause::NotOffered);
    assert!(!alix.client.attachments().offered());
    assert!(alix.client.attachments().list_pending().await?.is_empty());
    let dir2 = tempfile::tempdir()?;
    tester!(bo, attachments_dir: dir2.path(), configured: |configuration: &mut ServerConfiguration| {
            offer(configuration);
            configuration.attachments.as_mut().unwrap().max_upload_bytes = 10;
        }, disable_workers);
    let error = bo
        .client
        .attachments()
        .create(bytes())
        .await
        .err()
        .expect("creation must fail");
    assert_eq!(error.cause, Cause::TooLarge);
    assert!(bo.client.attachments().list_pending().await?.is_empty());
}

// verifies: ATCH-030
#[xmtp_common::test(unwrap_try = true)]
async fn exact_upload_limit_is_allowed() {
    let dir = tempfile::tempdir()?;
    let limit = ciphertext_len(encoded_prefix(Some("note.txt"), "text/plain", 18).len(), 18);
    tester!(alix, attachments_dir: dir.path(), configured: move |configuration: &mut ServerConfiguration| {
            offer(configuration);
            configuration.attachments.as_mut().unwrap().max_upload_bytes = limit;
        }, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    assert_eq!(
        pending.remote_attachment().content_length,
        Some(limit as u32)
    );
}

// verifies: ATCH-030
#[xmtp_common::test(unwrap_try = true)]
async fn create_retained_fields_limit() {
    use xmtp_content_types::{
        ContentCodec as _,
        attachment::{Attachment, AttachmentCodec},
    };
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

    let retained_len = |filename: Option<&str>, mime_type: &str| {
        let encoded = AttachmentCodec::encode(Attachment {
            filename: filename.map(str::to_owned),
            mime_type: mime_type.to_owned(),
            content: Vec::new(),
        })
        .expect("attachment encoding has no failure path");
        EncodedContent {
            r#type: encoded.r#type,
            parameters: encoded.parameters,
            compression: encoded.compression,
            ..Default::default()
        }
        .encoded_len()
    };
    let mut low: usize = 0;
    let mut high: usize = 65_536;
    while low < high {
        let middle = (low + high).div_ceil(2);
        if retained_len(Some(&"f".repeat(middle)), "text/plain") <= 65_536 {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let filename = "f".repeat(low);
    assert_eq!(retained_len(Some(&filename), "text/plain"), 65_536);
    assert_eq!(
        retained_len(Some(&format!("{filename}f")), "text/plain"),
        65_537
    );

    let allowed_dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: allowed_dir.path(), configured: offer, disable_workers);
    alix.client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"x".to_vec(),
            filename: Some(filename.clone()),
            mime_type: "text/plain".into(),
        })
        .await?;

    let denied_dir = tempfile::tempdir()?;
    tester!(bo, attachments_dir: denied_dir.path(), configured: offer, disable_workers);
    for (filename, mime_type) in [
        (Some(format!("{filename}f")), "text/plain".to_owned()),
        (None, "m".repeat(65_536)),
    ] {
        let error = bo
            .client
            .attachments()
            .create(AttachmentSource::Bytes {
                bytes: b"x".to_vec(),
                filename,
                mime_type,
            })
            .await
            .err()
            .expect("oversized retained fields must fail");
        assert_eq!(error.cause, Cause::TooLarge);
        assert!(bo.client.attachments().list_pending().await?.is_empty());
        assert!(bo.client.attachments().list_local().await?.is_empty());
        assert!(
            bo.client
                .context
                .attachment_runtime()
                .local_store()?
                .list_files()
                .await?
                .is_empty()
        );
    }
}

// verifies: ATCH-030
#[xmtp_common::test(unwrap_try = true)]
async fn empty_retained_values_use_decoder_limit() {
    let allowed_dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: allowed_dir.path(), configured: offer, disable_workers);
    let denied_dir = tempfile::tempdir()?;
    tester!(bo, attachments_dir: denied_dir.path(), configured: offer, disable_workers);

    for empty_mime in [false, true] {
        let fields = |length: usize| {
            if empty_mime {
                (Some("f".repeat(length)), String::new())
            } else {
                (Some(String::new()), "m".repeat(length))
            }
        };
        let fits = |length| {
            let (filename, mime_type) = fields(length);
            AttachmentDecoder::new()
                .push(&encoded_prefix(filename.as_deref(), &mime_type, 0))
                .is_ok()
        };
        let mut low = 0usize;
        let mut high = 65_536usize;
        while low < high {
            let middle = (low + high).div_ceil(2);
            if fits(middle) {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        assert!(fits(low));
        assert!(!fits(low + 1));

        let (filename, mime_type) = fields(low);
        let pending = alix
            .client
            .attachments()
            .create(AttachmentSource::Bytes {
                bytes: b"x".to_vec(),
                filename: filename.clone(),
                mime_type: mime_type.clone(),
            })
            .await?;
        let remote = pending.remote_attachment();
        let staged = tokio::fs::read(
            allowed_dir
                .path()
                .join(staged_path(&remote.content_digest)?),
        )
        .await?;
        let mut decrypted = Vec::new();
        let mut decryptor = GcmDecryptor::new(&KeyMaterial::from_remote(remote)?);
        decryptor.update(&staged, &mut decrypted)?;
        decryptor.finish()?;
        let mut decoder = AttachmentDecoder::new();
        let mut content = Vec::new();
        for bytes in decrypted.chunks(8192) {
            for event in decoder.push(bytes)? {
                match event {
                    xmtp_attachments::ContentChunk::Reset => content.clear(),
                    xmtp_attachments::ContentChunk::Bytes(bytes) => {
                        content.extend_from_slice(bytes);
                    }
                }
            }
        }
        let decoded = decoder.finish(&mut std::io::Cursor::new(&content), &mut Vec::new())?;
        assert_eq!(decoded.filename, filename);
        assert_eq!(decoded.mime_type, mime_type);
        assert_eq!(content, b"x");

        let (filename, mime_type) = fields(low + 1);
        let error = bo
            .client
            .attachments()
            .create(AttachmentSource::Bytes {
                bytes: b"x".to_vec(),
                filename,
                mime_type,
            })
            .await
            .err()
            .expect("oversized retained fields must fail");
        assert_eq!(error.cause, Cause::TooLarge);
        assert!(bo.client.attachments().list_pending().await?.is_empty());
        assert!(bo.client.attachments().list_local().await?.is_empty());
        assert!(
            bo.client
                .context
                .attachment_runtime()
                .local_store()?
                .list_files()
                .await?
                .is_empty()
        );
    }
}

// verifies: ATCH-032, ATCH-011
#[xmtp_common::test(unwrap_try = true)]
async fn source_moved_after_create() {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("source.txt");
    tokio::fs::write(&source, b"saved source").await?;
    tester!(alix, attachments_dir: dir.path().join("attachments"), disable_workers);
    let pending = alix
        .client
        .attachments()
        .create(AttachmentSource::Path {
            path: source.clone(),
            filename: None,
            mime_type: "text/plain".into(),
        })
        .await?;
    tokio::fs::rename(source, dir.path().join("moved.txt")).await?;
    assert_eq!(
        pending.remote_attachment().filename.as_deref(),
        Some("source.txt")
    );
    assert_eq!(
        tokio::fs::read(pending.local_path()?).await?,
        b"saved source"
    );
    pending.upload().await?;
    assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
}

// verifies: ATCH-033, ATCH-060
#[xmtp_common::test(unwrap_try = true)]
async fn failed_create_leaves_nothing() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let error = alix
        .client
        .attachments()
        .create(AttachmentSource::Path {
            path: dir.path().join("absent"),
            filename: None,
            mime_type: "text/plain".into(),
        })
        .await
        .err()
        .expect("creation must fail");
    assert_eq!(error.cause, Cause::SourceUnreadable);
    assert!(alix.client.attachments().list_pending().await?.is_empty());
    assert!(
        tokio::fs::read_dir(dir.path())
            .await?
            .next_entry()
            .await?
            .is_none()
    );
}

// verifies: ATCH-033, ATCH-060
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn fifo_source_is_rejected_before_staging() {
    use std::os::unix::ffi::OsStrExt as _;

    let dir = tempfile::tempdir()?;
    let fifo = dir.path().join("source.fifo");
    let fifo_name = std::ffi::CString::new(fifo.as_os_str().as_bytes())?;
    assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
    tester!(alix, attachments_dir: dir.path().join("attachments"), configured: offer, disable_workers);
    let client = alix.client.clone();
    let source = fifo.clone();
    let mut creating = tokio::spawn(async move {
        client
            .attachments()
            .create(AttachmentSource::Path {
                path: source,
                filename: None,
                mime_type: "text/plain".into(),
            })
            .await
    });
    let result = match tokio::time::timeout(Duration::from_millis(500), &mut creating).await {
        Ok(result) => result?,
        Err(_) => {
            let writer = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                let mut file = std::fs::OpenOptions::new().write(true).open(fifo)?;
                std::io::Write::write_all(&mut file, b"x")
            });
            tokio::time::timeout(Duration::from_secs(3), writer).await???;
            let _ = tokio::time::timeout(Duration::from_secs(3), creating).await?;
            panic!("FIFO source blocked creation");
        }
    };
    assert_eq!(
        result.err().expect("FIFO source must fail").cause,
        Cause::SourceUnreadable
    );
    assert!(alix.client.attachments().list_pending().await?.is_empty());
    assert!(alix.client.attachments().list_local().await?.is_empty());
    assert!(
        alix.client
            .context
            .attachment_runtime()
            .local_store()?
            .list_files()
            .await?
            .is_empty()
    );
}
