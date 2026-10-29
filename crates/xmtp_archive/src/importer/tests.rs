use super::*;
use crate::{MAX_FRAME_BYTES, ZSTD_WINDOW_LOG_MAX, exporter::ElementWriter};
use aes_gcm::{Aes256Gcm, KeyInit};
use async_compression::{Level, futures::write::ZstdEncoder, zstd::CParameter};
use futures_util::{
    AsyncWriteExt,
    io::{BufReader, Cursor},
};
use xmtp_proto::xmtp::device_sync::{BackupMetadataSave, consent_backup::ConsentSave};

const KEY: [u8; crate::ENC_KEY_SIZE] = [7; crate::ENC_KEY_SIZE];
const NONCE: [u8; NONCE_SIZE] = [0; NONCE_SIZE];

fn reader(bytes: Vec<u8>) -> AsyncReader {
    Box::pin(BufReader::new(Cursor::new(bytes)))
}

fn metadata() -> BackupElement {
    BackupElement {
        element: Some(Element::Metadata(BackupMetadataSave::default())),
    }
}

fn consent(entity: &str) -> BackupElement {
    BackupElement {
        element: Some(Element::Consent(ConsentSave {
            entity: entity.to_owned(),
            ..Default::default()
        })),
    }
}

fn encrypted_frame(plain: &[u8], nonce: &[u8; NONCE_SIZE]) -> Vec<u8> {
    #[allow(deprecated)]
    let cipher = Aes256Gcm::new(GenericArray::from_slice(&KEY));
    #[allow(deprecated)]
    let ciphertext = cipher
        .encrypt(GenericArray::from_slice(nonce), plain)
        .unwrap();
    let mut frame = (ciphertext.len() as u32).to_le_bytes().to_vec();
    frame.extend_from_slice(&ciphertext);
    frame
}

async fn archive(frames: &[u8]) -> Vec<u8> {
    let mut encoder = ZstdEncoder::new(Vec::new());
    encoder.write_all(frames).await.unwrap();
    encoder.close().await.unwrap();
    let mut bytes = crate::BACKUP_VERSION.to_le_bytes().to_vec();
    bytes.extend_from_slice(&NONCE);
    bytes.extend_from_slice(&encoder.into_inner());
    bytes
}

async fn archive_after_metadata(extra: &[u8]) -> Vec<u8> {
    let mut frames = encrypted_frame(&metadata().encode_to_vec(), &NONCE);
    frames.extend_from_slice(extra);
    archive(&frames).await
}

async fn assert_terminal_error(bytes: Vec<u8>, is_expected: impl Fn(&ArchiveError) -> bool) {
    let mut importer = ArchiveImporter::load(reader(bytes), &KEY).await.unwrap();
    let result = importer.next().await;
    assert!(
        matches!(result, Some(Err(ref error)) if is_expected(error)),
        "{result:?}"
    );
    assert!(importer.next().await.is_none(), "error was not terminal");
    assert!(
        importer.next().await.is_none(),
        "stream resumed after error"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn rejects_later_version_before_nonce_or_frame_read() {
    // verifies: ARCH-003. There is no nonce or frame after the version bytes.
    for version in [1u16, u16::MAX] {
        let result = ArchiveImporter::load(reader(version.to_le_bytes().to_vec()), &KEY).await;
        let error = result.err()?;
        assert!(matches!(error, ArchiveError::UnsupportedVersion(v) if v == version));
        assert!(error.to_string().contains(&version.to_string()));
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn rejects_incomplete_header_and_invalid_zstd() {
    // verifies: ARCH-018.
    let valid = archive(&encrypted_frame(&metadata().encode_to_vec(), &NONCE)).await;
    for length in 0..2 + NONCE_SIZE {
        let result = ArchiveImporter::load(reader(valid[..length].to_vec()), &KEY).await;
        assert!(
            matches!(result, Err(ArchiveError::IO(_))),
            "header length {length}"
        );
    }
    let mut invalid_zstd = valid[..2 + NONCE_SIZE].to_vec();
    invalid_zstd.extend_from_slice(&[0, 1, 2, 3, 4]);
    assert!(
        ArchiveImporter::load(reader(invalid_zstd), &KEY)
            .await
            .is_err()
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn protobuf_errors_are_terminal() {
    // verifies: ARCH-018.
    let bad_protobuf = encrypted_frame(&[0xff], &NONCE);
    assert_terminal_error(archive_after_metadata(&bad_protobuf).await, |error| {
        matches!(error, ArchiveError::Decode(_))
    })
    .await;
}

#[xmtp_common::test(unwrap_try = true)]
async fn incomplete_zstd_after_metadata_is_terminal() {
    // verifies: ARCH-018.
    let extra = encrypted_frame(
        &consent("one").encode_to_vec(),
        &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );
    let mut bytes = archive_after_metadata(&extra).await;
    bytes.pop();
    assert_terminal_error(bytes, |error| matches!(error, ArchiveError::IO(_))).await;
}

#[xmtp_common::test(unwrap_try = true)]
async fn complete_boundary_eof_and_both_nonce_forms() {
    // verifies: ARCH-002, ARCH-018.
    for legacy in [false, true] {
        let records = [consent("one"), consent("two")];
        let mut frames = encrypted_frame(&metadata().encode_to_vec(), &NONCE);
        let mut nonce = NONCE;
        for record in &records {
            if !legacy {
                nonce[0] += 1;
            }
            frames.extend_from_slice(&encrypted_frame(&record.encode_to_vec(), &nonce));
        }
        let mut importer = ArchiveImporter::load(reader(archive(&frames).await), &KEY).await?;
        for record in records {
            assert_eq!(importer.next().await.unwrap()?, record);
        }
        assert!(importer.next().await.is_none());
        assert!(importer.next().await.is_none());
    }

    let metadata_only = archive(&encrypted_frame(&metadata().encode_to_vec(), &NONCE)).await;
    let mut importer = ArchiveImporter::load(reader(metadata_only), &KEY).await?;
    assert!(importer.next().await.is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn frame_ceiling_bounds_reader_and_writer() {
    // verifies: ARCH-001, ARCH-018.
    assert_eq!(MAX_FRAME_BYTES, 75_497_472);
    let target = MAX_FRAME_BYTES - TAG_SIZE;
    let overhead = consent(&"a".repeat(target)).encoded_len() - target;
    let largest = consent(&"a".repeat(target - overhead));
    assert_eq!(largest.encoded_len(), target);
    let mut bytes = Vec::new();
    let mut writer = ElementWriter::new(&KEY, &mut bytes)?;
    writer.write(metadata().element?)?;
    writer.write(largest.element.clone()?)?;
    writer.finish()?;
    let mut importer = ArchiveImporter::load(reader(bytes), &KEY).await?;
    assert_eq!(importer.next().await??, largest);
    assert!(importer.next().await.is_none());

    let oversized = consent(&"a".repeat(target - overhead + 1)).element?;
    let mut writer = ElementWriter::new(&KEY, std::io::sink())?;
    let result = writer.write(oversized);
    assert!(
        matches!(result, Err(ArchiveError::InvalidFrame(_))),
        "{result:?}"
    );

    // The length is refused before the declared ciphertext is buffered.
    let mut frame = (MAX_FRAME_BYTES as u32 + 1).to_le_bytes().to_vec();
    frame.resize(4 + MAX_FRAME_BYTES + 1, 0);
    let mut importer =
        ArchiveImporter::load(reader(archive_after_metadata(&frame).await), &KEY).await?;
    let result = importer.next().await;
    assert!(
        matches!(result, Some(Err(ArchiveError::InvalidFrame(_)))),
        "{result:?}"
    );
    assert!(importer.decoded.len() < 2048, "{}", importer.decoded.len());
}

#[xmtp_common::test(unwrap_try = true)]
async fn rejects_zstd_window_above_limit() {
    // verifies: ARCH-018.
    assert_eq!(1 << ZSTD_WINDOW_LOG_MAX, 16_777_216);
    for window_log in [ZSTD_WINDOW_LOG_MAX + 1, ZSTD_WINDOW_LOG_MAX] {
        let params = [CParameter::window_log(window_log)];
        let mut encoder = ZstdEncoder::with_quality_and_params(Vec::new(), Level::Default, &params);
        encoder
            .write_all(&encrypted_frame(&metadata().encode_to_vec(), &NONCE))
            .await?;
        encoder.close().await?;
        let mut bytes = crate::BACKUP_VERSION.to_le_bytes().to_vec();
        bytes.extend_from_slice(&NONCE);
        bytes.extend_from_slice(&encoder.into_inner());
        let result = ArchiveImporter::load(reader(bytes), &KEY).await;
        if window_log > ZSTD_WINDOW_LOG_MAX {
            assert!(
                matches!(result, Err(ArchiveError::IO(_))),
                "window log {window_log}"
            );
        } else {
            assert!(result.is_ok(), "window log {window_log}");
        }
    }
}
