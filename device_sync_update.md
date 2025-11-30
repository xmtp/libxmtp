# Device Sync V2: Incremental Sync Architecture

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Current Solution](#current-solution)
3. [Proposed Solution](#proposed-solution)
4. [Sync Identity](#sync-identity)
5. [Encryption Scheme](#encryption-scheme)
6. [Client API](#client-api)
7. [Server API](#server-api)

---

## Executive Summary

This document proposes a comprehensive redesign of the device sync system. The new architecture delivers:

- **Sub-second initial sync** vs. potentially minutes for full history
- **On-demand message loading** per conversation
- **Resumable transfers** - failed uploads/downloads resume from last position, not restart
- **Zero-knowledge server** that cannot correlate data to user identities
- **Forward secrecy** with proper key rotation
- **Incremental sync** reducing bandwidth by 90%+ for typical usage
- **Explicit API control** - all sync operations triggered via explicit function calls

The redesign transforms device sync from an opaque, error-prone, all-or-nothing operation into a fast, reliable system with explicit developer control that enables a much smoother user experience.

---

## Current Solution

### Architecture

The current device sync uses a request-response model coordinated through an MLS sync group. When a new installation comes online, it automatically sends a `DeviceSyncRequest`. An existing installation must explicitly sync to receive this request, then sends a `DeviceSyncAcknowledge` to claim responsibility for the upload. The first installation to acknowledge "wins" and proceeds to create a full archive of all data, encrypt it with a random key, upload it to the sync server, and send a `DeviceSyncReply` containing the download URL and decryption key. The new installation must then explicitly sync again to receive the reply and download the archive.

Note: The acknowledgement mechanism has a race condition - if two installations acknowledge simultaneously, both will create and upload archives, wasting bandwidth and potentially causing consistency issues.

### Key Limitations Summary

| Issue                      | Impact                                                                                                |
| -------------------------- | ----------------------------------------------------------------------------------------------------- |
| **No incremental sync**    | Must upload/download full history (potentially 100MB+) on every sync                                  |
| **No transfer resilience** | Failed transfers must restart from beginning with no automatic retry; failures are silently swallowed |
| **Opaque process**         | Syncing requires a specific function call; no visibility into sync state or progress                  |
| **No transfer size info**  | Cannot determine bandwidth requirements before sync; can be problematic on metered connections        |

### User Experience Problems

1. **New device setup can take minutes** - User waits for sync without any indication of progress
2. **Can't see conversations immediately** - Must wait for full download
3. **Unknown data usage** - May incur unexpected costs
4. **Silent failures** - If upload or download fails, user is unaware and stuck without history; manual retry required

---

## Proposed Solution

### Architecture

The proposed solution decouples data transfer from sync group updates. Installations share a `SyncIdentity` (containing server credentials and encryption keys) through the MLS sync group. Each installation can independently sync consent, group, and message data with the sync server.

### Key Improvement Summary

| Aspect                    | Current                   | Proposed                                                               |
| ------------------------- | ------------------------- | ---------------------------------------------------------------------- |
| Initial sync time         | Potentially minutes       | < 1 second                                                             |
| Initial sync size         | Entire history            | Metadata only (varies by group count/metadata size)                    |
| Message loading           | All at once               | Per-conversation, on-demand                                            |
| Server identity knowledge | Anonymous uploads         | Opaque sync_id with auth                                               |
| Archive encryption key    | Random per-request        | Derived KEK shared via MLS sync group; rotated on installation changes |
| Incremental updates       | No                        | Yes                                                                    |
| Resumable downloads       | No                        | Yes (byte-range support)                                               |
| Resumable uploads         | No                        | Yes                                                                    |
| Forward secrecy           | Yes (random key per sync) | Yes (KEK rotated on installation changes)                              |
| Transfer size visibility  | Unknown until complete    | Exact sizes known upfront for bandwidth planning                       |
| API control               | Opaque background worker  | Explicit function calls with error handling                            |

### User Experience Improvements

1. **Instant conversation list** - See all chats in < 1 second
2. **Progressive loading** - Messages load when you open a conversation
3. **Low bandwidth** - Only downloads what you need
4. **Resumable downloads** - Interrupted transfers resume from last position, not restart
5. **Error visibility** - Sync failures are surfaced to the user with actionable feedback

### Developer Experience Improvements

1. **Explicit control** - All sync operations triggered via function calls
2. **Error handling** - Functions return `Result<T, E>` for proper error propagation
3. **Progress callbacks** - Optional callbacks for upload/download progress
4. **Cancellation support** - Ability to cancel in-progress sync operations
5. **Predictable behavior** - No surprising background activity or hidden state changes

---

## Sync Identity

The MLS sync group is the secure channel through which installations share encryption keys and coordinate sync operations. It is separate from the sync server - the sync group handles key distribution while the server handles data storage.

### Distribution

When a new installation joins an inbox, it sends a `SyncIdentityRequest` to the MLS sync group. An existing installation acknowledges the request and generates a new `SyncIdentity` with a rotated KEK for forward secrecy. All installations receive the new identity and store it in their local DB for use when syncing with the sync server.

```rust
/// Sync identity - distributed via MLS sync group, stored in local DB
struct SyncIdentity {
    /// Random 32-byte identifier - no connection to inbox_id
    sync_id: [u8; 32],
    /// Ed25519 keypair for authenticating with sync server
    auth_keypair: Ed25519Keypair,
    /// Current Key Encryption Key for archives
    kek: [u8; 32],
    kek_version: u64,
    /// Creation timestamp
    created_at_ns: i64,
}

/// Request sync identity (sent by new installation)
struct SyncIdentityRequest {
    /// Random ID to correlate request with acknowledgement
    request_id: [u8; 32],
}

/// Acknowledge request (sent by leader before generating new identity)
struct SyncIdentityAcknowledge {
    /// Matches the request_id from SyncIdentityRequest
    request_id: [u8; 32],
}
```

### Rotation on Revocation

When an installation is revoked, the sync identity must be rotated to ensure forward secrecy - the revoked installation should not be able to decrypt future sync data. Once rotated, previous sync data will not be accessible either.

Rotation uses a two-phase approach to handle failures:

```rust
/// Phase 1: Claim rotation responsibility and share new identity
struct SyncIdentityRotationClaim {
    /// Random ID to identify this rotation attempt
    rotation_id: [u8; 32],
    /// The new sync identity (shared preemptively)
    new_identity: SyncIdentity,
}

/// Phase 2: Confirm rotation completed on server
struct SyncIdentityRotationConfirm {
    /// Matches the rotation_id from SyncIdentityRotationClaim
    rotation_id: [u8; 32],
}
```

**Flow:**

1. Installation detects revocation, generates new `SyncIdentity`
2. Broadcasts `SyncIdentityRotationClaim` with the new identity
3. All installations store the new identity (preemptively)
4. Claiming installation rotates keys on sync server
5. Broadcasts `SyncIdentityRotationConfirm` on success

**Failure handling:**

| Failure Point                     | State                                      | Recovery                                                                               |
| --------------------------------- | ------------------------------------------ | -------------------------------------------------------------------------------------- |
| Before claim sent                 | No change                                  | Another installation can claim                                                         |
| Claim sent, server rotation fails | All have new identity, server has old keys | Claimer retries server rotation with same identity                                     |
| Server rotated, confirm not sent  | All have new identity, server has new keys | System is functional; confirm is optional verification                                 |
| Claim sent, claimer goes offline  | All have new identity, server has old keys | After timeout, another installation retries server rotation using the claimed identity |

**Key insight:** By broadcasting the new identity _before_ server rotation, we ensure all installations have the keys needed to decrypt regardless of where the process fails. The worst case is the server still has old keys, which can be retried. If a claim is received but no confirmation follows, installations can ignore the claim and another installation can attempt rotation.

**Note:** The blobs themselves don't move - they're content-addressed and shared globally. Only the manifest is updated with re-wrapped DEKs.

### Offline Installation Recovery

If an installation is offline during a KEK rotation, it recovers through the MLS sync group message history.

### Stale Data Handling

When server data is stale, new installations can request updates from peers via the MLS sync group.

```rust
/// Request peers to upload their latest data to the sync server
struct SyncDataRequest {
    /// Random ID to correlate request with acknowledgement
    request_id: [u8; 32],
}

/// Acknowledge sync data request (sent by installation handling the upload)
struct SyncDataAcknowledge {
    /// Matches the request_id from SyncDataRequest
    request_id: [u8; 32],
}
```

---

## Encryption Scheme

- Each archive has a unique random DEK
- DEKs are wrapped with KEK
- On rotation, all DEKs are re-wrapped with the new KEK
- Installations with old KEK cannot decrypt current archives

### Privacy Model

Instead of using `inbox_id` (which links to on-chain identity), we generate a random `sync_id` that is:

- Unlinkable to XMTP identity
- Distributed only via MLS-encrypted sync group
- Rotated on every installation change

### Manifest

There is exactly one manifest per sync_id, stored as `{sync_id}.manifest`. The manifest is the encrypted index that makes content blobs useful - without it, blobs are opaque and undecryptable since the manifest contains the wrapped DEKs needed to decrypt each blob.

The manifest will typically be in 2-10 KB range, but grows with number of groups/blobs.

```rust
/// Stored on server - encrypted, opaque to server
struct EncryptedManifest {
    /// DEK for manifest, wrapped with KEK
    wrapped_manifest_dek: Vec<u8>,
    /// KEK version used for wrapping
    kek_version: u64,
    /// Encrypted manifest (AES-256-GCM)
    ciphertext: Vec<u8>,
    /// Random 12-byte nonce for AES-256-GCM decryption
    nonce: [u8; 12],
}

/// Decrypted manifest - only visible to client
struct Manifest {
    /// The inbox_id this manifest belongs to
    inbox_id: String,
    /// Timestamp of last manifest update
    last_updated_ns: i64,
    /// Archive entry for consent records
    consent: ArchiveEntry,
    /// Archive entry for group metadata
    groups: ArchiveEntry,
    /// Per-group message archives, keyed by group_id
    group_messages: HashMap<GroupId, Vec<MessageArchiveEntry>>,
}

struct ArchiveEntry {
    /// SHA-256 hash of encrypted blob as hex string (used as filename on server)
    content_hash: String,
    /// DEK wrapped with KEK
    wrapped_dek: Vec<u8>,
    /// Size of encrypted blob in bytes
    size_bytes: u64,
    /// When this entry was created
    created_at_ns: i64,
}

struct MessageArchiveEntry {
    /// SHA-256 hash of encrypted blob as hex string (used as filename on server)
    content_hash: String,
    /// DEK wrapped with KEK
    wrapped_dek: Vec<u8>,
    /// Size of encrypted blob in bytes
    size_bytes: u64,
    /// When this entry was created
    created_at_ns: i64,
    /// Time range of messages in this archive
    time_range_start_ns: i64,
    time_range_end_ns: i64,
    /// Number of messages in this archive
    message_count: u64,
}
```

### Content Blobs

Content blobs are encrypted archives stored by their content hash. They are shared globally across all users - the server cannot determine ownership.

| Type     | Contents               | Typical Size |
| -------- | ---------------------- | ------------ |
| Consent  | All consent records    | 5-20 KB      |
| Groups   | All group metadata     | 10KB-1MB+    |
| Messages | Messages for one group | 100KB-2MB    |

Blobs are immutable and content-addressed:

- Filename is SHA-256 hash of the encrypted content
- Same content = same hash = stored once
- Blobs are only useful with the corresponding manifest entry

### Key Wrapping

Each archive has a unique, random Data Encryption Key (DEK). The DEK is wrapped (encrypted) with the KEK:

```rust
/// Creates an encrypted archive from data.
///
/// Steps:
/// 1. Generate random 32-byte DEK
/// 2. Generate random 12-byte nonce
/// 3. Encrypt data with AES-256-GCM using DEK and nonce
/// 4. Wrap DEK with KEK using AES key wrap (RFC 3394)
/// 5. Return EncryptedArchive and ArchiveEntry (metadata + wrapped DEK)
fn create_archive(data: &[u8], kek: &[u8]) -> (EncryptedArchive, ArchiveEntry);

/// Decrypts an encrypted archive using wrapped DEK and KEK.
fn decrypt_archive(archive: &EncryptedArchive, wrapped_dek: &[u8], kek: &[u8]) -> Vec<u8>;
```

### Key Rotation

Key rotation occurs when installations are added or revoked.

- Archives (blobs) are NOT re-uploaded or moved
- New manifest created
- Old manifest removed

---

## Client API

### Public API

```rust
impl SyncClient {
    // ============ Download Operations ============

    /// Download manifest from sync server.
    /// Required for syncing consent, groups, and messages.
    pub async fn download_manifest(&self) -> Result<SyncManifest, SyncError>;

    /// Download and import consent records.
    pub async fn sync_consent(
        &self,
        manifest: &SyncManifest,
    ) -> Result<SyncDownloadResult, SyncError>;

    /// Download and import group metadata (names, settings, etc.).
    pub async fn sync_groups(
        &self,
        manifest: &SyncManifest,
    ) -> Result<SyncDownloadResult, SyncError>;

    /// Download and import messages for a specific group.
    pub async fn sync_group_messages(
        &self,
        group_id: &GroupId,
        manifest: &SyncManifest,
        opts: SyncDownloadOptions,
    ) -> Result<SyncDownloadResult, SyncError>;

    /// Download and import all message history for all groups.
    pub async fn sync_all_messages(
        &self,
        manifest: &SyncManifest,
        opts: SyncDownloadOptions,
    ) -> Result<SyncDownloadResult, SyncError>;

    /// Calculate the exact download size for a sync operation without downloading.
    /// Use this to inform users of bandwidth requirements before starting.
    pub fn calculate_download_size(
        &self,
        manifest: &SyncManifest,
        scope: SyncScope,
    ) -> SyncTransferSize;

    // ============ Upload Operations ============

    /// Upload local consent records to sync server.
    /// Call periodically or after consent changes.
    pub async fn upload_consent(&self) -> Result<SyncUploadResult, SyncError>;

    /// Upload local group metadata to sync server.
    /// Call periodically or after group changes.
    pub async fn upload_groups(&self) -> Result<SyncUploadResult, SyncError>;

    /// Upload messages for a specific group.
    /// Call periodically or after sending messages.
    pub async fn upload_group_messages(
        &self,
        group_id: &GroupId,
    ) -> Result<SyncUploadResult, SyncError>;

    /// Upload all pending changes (consent, groups, messages).
    /// Convenience method that calls all upload functions.
    pub async fn upload_all(&self) -> Result<SyncUploadResult, SyncError>;

    /// Resume any uploads that were interrupted (e.g., app killed, network lost).
    /// Call on app startup to complete partial uploads from previous session.
    pub async fn resume_pending_uploads(&self) -> Result<SyncResumeResult, SyncError>;

    /// Calculate the exact upload size for pending changes without uploading.
    /// Use this to inform users of bandwidth requirements before starting.
    pub async fn calculate_upload_size(&self) -> Result<SyncTransferSize, SyncError>;

    // ============ Identity Management ============

    /// Performs full identity rotation (triggered on installation add/revoke).
    ///
    /// Steps performed:
    /// 1. Generate completely new SyncIdentity (new sync_id, auth keypair, KEK)
    /// 2. Download and decrypt current manifest from server
    /// 3. Re-wrap all DEKs (consent, groups, all message archives) with new KEK
    /// 4. Encrypt manifest with new KEK
    /// 5. Send atomic rotation request to server (RotateRequest with old signature)
    /// 6. Handle response
    ///    - On success: Broadcast IdentityRotation message to sync group, update local sync_identity
    ///    - On ConcurrentRotation error: Another installation beat us; sync the group to get
    ///      their IdentityRotation message and process it instead
    ///    - On other errors: Propagate error to caller
    /// 7. Update local sync_identity to new identity
    pub async fn rotate_identity(&self) -> Result<(), SyncError>;

    /// Get current sync status and statistics.
    pub fn status(&self) -> SyncStatus;

    // ============ Data Management ============

    /// Delete all sync data from the server (manifest and all blobs).
    /// This is a destructive operation - other installations will lose access to
    /// synced history until data is re-uploaded.
    /// Local data is NOT affected.
    pub async fn delete_all_server_data(&self) -> Result<(), SyncError>;

    /// Get detailed breakdown of sync storage usage on the server.
    /// Requires downloading the manifest to compute sizes.
    pub async fn storage_usage(&self) -> Result<SyncStorageUsage, SyncError>;
}

/// Options for download operations
pub struct SyncDownloadOptions {
    /// Only download messages after this timestamp
    pub after_ns: Option<i64>,
    /// Only download messages before this timestamp
    pub before_ns: Option<i64>,
    /// Progress callback (bytes_transferred, total_bytes)
    pub on_progress: Option<Box<dyn Fn(u64, u64) + Send>>,
    /// Cancellation token
    pub cancel_token: Option<CancellationToken>,
}

/// Scope of a sync operation for size calculation
pub enum SyncScope {
    /// Just consent records
    Consent,
    /// Just group metadata
    Groups,
    /// Messages for a specific group
    GroupMessages(GroupId),
    /// All messages for all groups
    AllMessages,
    /// Everything (consent + groups + all messages)
    Everything,
}

/// Transfer size information for bandwidth planning
pub struct SyncTransferSize {
    /// Total bytes to transfer
    pub total_bytes: u64,
    /// Breakdown by category
    pub consent_bytes: u64,
    pub groups_bytes: u64,
    pub messages_bytes: u64,
    /// Number of archives involved
    pub archive_count: u64,
}

/// Result of a sync operation
pub struct SyncDownloadResult {
    /// Number of records imported
    pub records_imported: u64,
    /// Number of records skipped
    pub records_skipped: u64,
    /// Bytes downloaded
    pub bytes_downloaded: u64,
}

pub struct SyncUploadResult {
    /// Number of records uploaded
    pub records_uploaded: u64,
    /// Bytes uploaded
    pub bytes_uploaded: u64,
    /// Whether manifest was updated
    pub manifest_updated: bool,
}

pub struct SyncResumeResult {
    /// Number of uploads successfully resumed and completed
    pub resumed: u64,
    /// Number of uploads that could not be resumed (will need to re-upload)
    pub failed: u64,
    /// Total bytes uploaded during resume
    pub bytes_uploaded: u64,
}

pub struct SyncStatus {
    /// Whether sync identity is initialized
    pub is_initialized: bool,
    /// Last successful manifest download
    pub last_manifest_sync_ns: Option<i64>,
    /// Number of groups with pending message uploads
    pub groups_pending_upload: u64,
    /// Total bytes pending upload
    pub bytes_pending_upload: u64,
}

/// Detailed breakdown of sync storage usage on the server
pub struct SyncSyncStorageUsage {
    /// Size of encrypted manifest
    pub manifest_bytes: u64,
    /// Size of consent archive
    pub consent_bytes: u64,
    /// Size of groups archive
    pub groups_bytes: u64,
    /// Total size of all message archives
    pub messages_total_bytes: u64,
    /// Per-group message storage breakdown
    pub messages_by_group: HashMap<GroupId, GroupSyncStorageUsage>,
    /// Total storage across all categories
    pub total_bytes: u64,
}

/// Storage usage for a single group's messages
pub struct GroupSyncStorageUsage {
    /// Total bytes for this group's message archives
    pub total_bytes: u64,
    /// Number of message archives for this group
    pub archive_count: u64,
    /// Total message count across all archives
    pub message_count: u64,
    /// Time range covered by archives
    pub time_range_start_ns: i64,
    pub time_range_end_ns: i64,
}

/// Client configuration for sync behavior
pub struct SyncConfig {
    /// When enabled, automatically upload local changes when a peer requests sync
    /// via the sync group. This ensures new installations get fresh data.
    /// Default: false (developer must explicitly call upload_all)
    pub auto_upload_on_request: bool,

    /// When enabled, periodically upload in the background at the specified interval.
    /// This keeps the server up-to-date for faster new device onboarding.
    /// Default: None (no automatic uploading)
    pub auto_upload_interval: Option<Duration>,

    /// Threshold for considering server data "stale". When downloading manifest,
    /// if last_updated_ns is older than this, the manifest is marked as stale.
    /// Default: 24 hours
    pub stale_threshold: Duration,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            auto_upload_on_request: false,
            auto_upload_interval: None,
            stale_threshold: Duration::from_secs(24 * 60 * 60),
        }
    }
}
```

### Handling Stale Syncs

A sync is considered "stale" when the server's manifest hasn't been updated recently. This occurs when active installations haven't uploaded their latest changes.

**Scenario:** Installation A has been active for a month without uploading. Installation B (new) syncs from the server but doesn't get the latest data.

**Solution:** Sync from server immediately, then request updates from peers if stale.

```rust
impl SyncClient {
    /// Returns true if the manifest is older than the given threshold.
    /// Compares (now_ns - last_updated_ns) against threshold.as_nanos().
    pub fn is_stale(&self, threshold: Duration) -> bool;

    /// Sends a SyncDataRequest message to the MLS sync group.
    /// All peer installations will receive this request.
    /// Peers with auto_upload_on_request enabled will respond by calling upload_all().
    /// Returns immediately after sending; peer uploads happen asynchronously.
    pub async fn request_update_from_peers(&self) -> Result<(), SyncError>;

    /// Handles incoming SyncDataRequest from a peer installation.
    ///
    /// Behavior:
    /// 1. If auto_upload_on_request is disabled in config, ignores the request and returns Ok.
    /// 2. If local data is newer than server, calls upload_all() to upload latest changes.
    /// 3. Returns Ok(()) regardless of whether upload was triggered.
    async fn handle_sync_data_request(&self, request: SyncDataRequest) -> Result<()>;
}
```

### Sync Flows

```rust
impl SyncClient {
    /// Performs fast initial sync of metadata only (consent + groups).
    ///
    /// Steps performed:
    /// 1. Download and decrypt manifest from server
    /// 2. Download consent blob using manifest.consent.content_hash, decrypt, import to local DB
    /// 3. Download groups blob using manifest.groups.content_hash, decrypt, import to local DB
    /// 4. Return SyncManifest for use in subsequent message sync operations
    ///
    /// Note: Manifest is not cached locally for security.
    pub async fn sync_metadata(&self) -> Result<SyncManifest>;
}
```

#### On-Demand Message Sync

```rust
impl SyncClient {
    /// Loads messages for a specific group on-demand (called when user opens conversation).
    ///
    /// Requires manifest from prior sync_download_manifest() call.
    ///
    /// Steps performed:
    /// 1. Look up group_id in manifest.group_messages; return Ok if not found
    /// 2. Sort archives by time_range_end_ns descending (most recent first)
    /// 3. For each archive not already imported (checked via db.has_imported):
    ///    a. Download blob from server using archive.content_hash
    ///    b. Decrypt archive using unwrapped DEK
    ///    c. Import messages to local DB
    ///    d. Mark archive as imported to avoid re-downloading
    pub async fn sync_group_messages(
        &self,
        group_id: &GroupId,
        manifest: &SyncManifest,
    ) -> Result<()>;
}
```

#### Resumable Downloads

Failed downloads can resume from the last successful byte position using HTTP Range requests:

```rust
impl SyncClient {
    /// Downloads a blob with HTTP Range resume support.
    ///
    /// Steps performed:
    /// 1. Check for existing partial download at {cache_dir}/{hash}.partial
    /// 2. If partial exists, read it and set start_byte = buffer.len()
    /// 3. Send GET request with "Range: bytes={start_byte}-" header if resuming
    /// 4. Handle response status:
    ///    - 200 OK: Full response, replace buffer with response body
    ///    - 206 Partial Content: Append response body to existing buffer
    ///    - 416 Range Not Satisfiable: Partial invalid, clear buffer and retry full download
    /// 5. Verify SHA-256 hash of complete buffer matches content_hash
    /// 6. Delete partial file on success, return buffer
    async fn download_blob_resumable(&self, content_hash: &ContentHash) -> Result<Vec<u8>>;

    /// Downloads a blob with automatic progress checkpointing.
    ///
    /// Steps performed:
    /// 1. Check for existing partial download, resume from last byte if found
    /// 2. Stream response body in chunks
    /// 3. Every 256KB, save current buffer to {cache_dir}/{hash}.partial
    /// 4. On completion, verify SHA-256 hash matches content_hash
    /// 5. Delete partial file and return complete buffer
    ///
    /// This ensures progress is preserved even if download is interrupted.
    async fn download_blob_with_progress(&self, content_hash: &ContentHash) -> Result<Vec<u8>>;
}
```

This approach provides:

- **Automatic resume** - Partial downloads are saved and resumed on retry
- **Progress preservation** - Every 256KB is checkpointed to disk
- **Hash verification** - Content hash ensures integrity after resume
- **Graceful fallback** - Works even if server doesn't support Range requests

#### Upload Flow

```rust
impl SyncClient {
    /// Uploads all local changes to the sync server.
    ///
    /// Steps performed:
    /// 1. Download and decrypt current manifest from server
    /// 2. Check if consent records changed since manifest.consent.created_at_ns:
    ///    - If changed, create encrypted consent archive, upload blob, update manifest entry
    /// 3. Check if groups changed since manifest.groups.created_at_ns:
    ///    - If changed, create encrypted groups archive, upload blob, update manifest entry
    /// 4. For each group with new messages since last sync:
    ///    - Create encrypted message archive, upload blob, update manifest entry
    /// 5. Encrypt and upload updated manifest
    pub async fn upload_changes(&self) -> Result<()>;

    /// Creates an encrypted message archive and uploads it to the server.
    ///
    /// Steps performed:
    /// 1. Serialize messages to bytes
    /// 2. Generate random 32-byte DEK
    /// 3. Generate random 12-byte nonce
    /// 4. Encrypt serialized data with AES-256-GCM using DEK and nonce
    /// 5. Compute SHA-256 content hash of ciphertext
    /// 6. Upload ciphertext blob to server (stored by content hash)
    /// 7. Create MessageArchiveEntry with:
    ///    - content_hash from step 5
    ///    - DEK wrapped with current KEK
    ///    - Metadata (size, time range, message count)
    /// 8. Return MessageArchiveEntry for manifest update
    async fn create_and_upload_message_archive(
        &self,
        group_id: &GroupId,
        messages: Vec<Message>,
    ) -> Result<MessageArchiveEntry>;
}
```

#### Resumable Uploads

Failed uploads can be retried with automatic resume support:

```rust
/// Pending upload tracked in local database for resume support
#[derive(Serialize, Deserialize)]
struct PendingUpload {
    /// SHA-256 hash of the complete blob (for integrity verification)
    content_hash: [u8; 32],
    /// Total size of the blob in bytes
    total_size: u64,
    /// Path to locally cached encrypted blob (needed for resume if app restarts)
    local_cache_path: PathBuf,
    /// Timestamp when this upload was initiated (nanoseconds since epoch)
    started_at_ns: i64,
}

impl SyncClient {
    /// Uploads a blob with retry support.
    ///
    /// Steps performed:
    /// 1. Compute SHA-256 content hash of data
    /// 2. Check local DB for existing pending upload with same content_hash:
    ///    - If found, use cached blob
    ///    - If not found, cache blob locally and store PendingUpload record
    /// 3. Upload with retry logic (exponential backoff)
    /// 4. Delete PendingUpload record and local cache file on success
    /// 5. Return content_hash
    async fn upload_blob_resumable(&self, data: &[u8]) -> Result<ContentHash>;

    /// Resumes any pending uploads from previous sessions.
    ///
    /// Called on app startup to complete interrupted uploads.
    ///
    /// Steps performed:
    /// 1. Load all PendingUpload records from local DB
    /// 2. For each pending upload:
    ///    a. Check if local cache file still exists; if not, mark as failed and skip
    ///    b. Read cached blob and retry upload
    /// 3. Return SyncResumeResult with count of resumed vs failed uploads
    pub async fn resume_pending_uploads(&self) -> Result<SyncResumeResult>;
}
```

This approach provides:

- **Automatic resume** - Pending uploads tracked in local DB and resumed on next sync
- **Local caching** - Encrypted blob cached locally until upload completes
- **Retry with backoff** - Transient failures automatically retried

---

## Server API

```
┌─────────────────────────────────────────────────────────────────┐
│  Sync Server API                                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Storage Model (Flat):                                          │
│  - Manifests: {sync_id}.manifest (one per user)                 │
│  - Blobs: {content_hash} (shared globally)                      │
│  - All files are encrypted blobs opaque to server               │
│                                                                 │
│  Authentication: Ed25519 signature on request                   │
│                                                                 │
│  ─────────────────────────────────────────────────────────────  │
│                                                                 │
│  POST /register                                                 │
│    Body: { sync_id, auth_public_key }                           │
│    → Registers sync_id with auth_public_key                     │
│                                                                 │
│  GET /{sync_id}.manifest                                        │
│    → Returns encrypted manifest                                 │
│                                                                 │
│  GET /blob/{content_hash}                                       │
│    Headers: Range (optional, e.g., "bytes=1024-2047")           │
│    → Returns blob by content hash                               │
│    → Response includes Content-Length, Accept-Ranges: bytes     │
│                                                                 │
│  PUT /{sync_id}.manifest                                        │
│    Body: encrypted manifest blob                                │
│    → Stores/updates manifest                                    │
│                                                                 │
│  PUT /blob/{content_hash}                                       │
│    Body: encrypted blob                                         │
│    → Stores blob (content-addressed, deduplicated)              │
│                                                                 │
│  POST /rotate/{old_sync_id}                                     │
│    Body: {                                                      │
│      new_sync_id,                                               │
│      new_auth_public_key,                                       │
│      new_manifest                                               │
│    }                                                            │
│    → Deletes {old_sync_id}.manifest                             │
│    → Stores {new_sync_id}.manifest                              │
│    → Updates auth_public_key                                    │
│                                                                 │
│  DELETE /{sync_id}.manifest                                     │
│    → Deletes manifest only (blobs are shared, not deleted)      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Server Implementation (Rotation)

```rust
impl SyncServer {
    /// Performs atomic identity rotation with concurrency control.
    ///
    /// Steps performed:
    /// 1. Verify request signature using old account's auth_public_key
    /// 2. Load current manifest and check version matches request.expected_version;
    ///    if version mismatch, return ConcurrentRotation error (another installation already rotated)
    /// 3. Execute atomic transaction:
    ///    - Delete {old_sync_id}.manifest
    ///    - Store {new_sync_id}.manifest
    ///    - Update auth record (old_sync_id -> new_sync_id, new_auth_public_key)
    /// 4. Commit transaction
    ///
    /// Note: Blobs are not touched - they're content-addressed and shared globally.
    async fn rotate(&self, old_sync_id: &str, request: RotateRequest) -> Result<()>;
}
```

---

### Performance Characteristics

| Operation             | Time (3G) | Time (WiFi) | Bandwidth |
| --------------------- | --------- | ----------- | --------- |
| Initial sync (groups) | 1-3 sec   | < 500ms     | ~100 KB   |
| Load one conversation | 2-5 sec   | < 1 sec     | 100KB-2MB |
| Full background sync  | 1-10 min  | 10-60 sec   | 10-100 MB |
| Identity rotation     | 1-2 sec   | < 500ms     | ~10 KB    |
| Upload new messages   | 1-3 sec   | < 500ms     | ~100 KB   |
