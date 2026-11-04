//! Device-sync internals that suites outside the crate check: the sync
//! message encoding, the outbox page limits, the incoming-update log and the
//! archive import steps.

use super::archive::ArchiveImporter;
use super::{DeviceSyncError, ReceivedContent};
use futures::Stream;
use xmtp_mls_engine::groups::{context::XmtpSharedContext, deletion::ArchivedSelfDeletions};
use xmtp_proto::xmtp::device_sync::BackupElement;
use xmtp_proto::xmtp::device_sync::content::{
    PreferenceUpdate, device_sync_content::Content as ContentProto,
};
use zeroize::Zeroizing;

/// The records per sync message the outbox sends.
pub const PAGE_UPDATES: usize = super::PAGE_UPDATES;
/// The bytes of encoded updates per sync message.
pub const PAGE_BYTES: usize = super::PAGE_BYTES;

/// As the sync worker encodes `content` as a sync message.
pub fn sync_message_bytes(content: ContentProto) -> Zeroizing<Vec<u8>> {
    super::sync_message_bytes(content)
}

/// As the sync worker decodes a received sync message.
pub fn decode_sync_message(plaintext: &[u8]) -> Option<ReceivedContent> {
    super::decode_sync_message(plaintext)
}

/// As the sync worker decodes the device-sync content inside a sync message.
pub fn decode_supported_content(bytes: &[u8]) -> Option<ReceivedContent> {
    super::decode_supported_content(bytes)
}

/// As the sync worker logs the preference updates it receives.
pub fn log_incoming_preference_updates(updates: &[PreferenceUpdate]) {
    super::worker::log_incoming_preference_updates(updates)
}

/// Import an archive read once, for tests. A pre-8.0 self-deletion is
/// restored only when the archive lists it before its target; imports use
/// [`insert_archive`](super::archive::insert_archive), which reads the archive twice and restores it in
/// either order.
pub async fn insert_importer(
    importer: &mut ArchiveImporter,
    context: &impl XmtpSharedContext,
) -> Result<(), DeviceSyncError> {
    super::archive::insert_elements(importer, context, ArchivedSelfDeletions::default()).await
}

/// As an archive import stores elements it read, with the archive's own
/// deletion messages already collected.
pub async fn insert_elements<S, E>(
    elements: &mut S,
    context: &impl XmtpSharedContext,
    self_deletions: ArchivedSelfDeletions,
) -> Result<(), DeviceSyncError>
where
    S: Stream<Item = Result<BackupElement, E>> + Unpin,
    DeviceSyncError: From<E>,
{
    super::archive::insert_elements(elements, context, self_deletions).await
}

/// As an archive import collects the archive's deletion messages before it
/// stores the archive.
pub async fn collect_self_deletions<S, E>(elements: &mut S) -> Result<ArchivedSelfDeletions, E>
where
    S: Stream<Item = Result<BackupElement, E>> + Unpin,
{
    super::archive::collect_self_deletions(elements).await
}
