# Device Sync V2: Incremental Sync Architecture

## Table of Contents

1. [Overview](#overview)
2. [Current Solution](#current-solution)
3. [Proposed Solution](#proposed-solution)
4. [Sync Identity](#sync-identity)
5. [Encryption Scheme](#encryption-scheme)
6. [Client API](#client-api)
7. [Server API (Web Provider)](#server-api-web-provider)

---

## Overview

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

The current device sync uses a request-response model coordinated through an MLS sync group. When a new installation comes online, it automatically sends a device sync request message. An existing installation must sync to receive this request, then sends a device sync acknowledge message to claim responsibility for the upload. The first installation to acknowledge "wins" and proceeds to create a full archive of all data, encrypt it with a random key, upload it to the sync server, and send a device sync reply message containing the download URL and decryption key. The new installation must then sync again to receive the reply and download the archive.

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

Like the current implementation, the sync group is used to coordinate device syncing across installations. The main difference is that data transfers are done independently and at the discretion of the developer or user. Installations use the sync group to share sync server credentials and encryption keys as installations are created and revoked. Each installation can independently sync consent, group, and message data with the sync server across different time spans.

### Key Improvement Summary

| Aspect                   | Current                   | Proposed                                            |
| ------------------------ | ------------------------- | --------------------------------------------------- |
| Initial sync time        | Potentially minutes       | < 1 second                                          |
| Initial sync size        | Entire history            | Metadata only (varies by group count/metadata size) |
| Message loading          | All at once               | Per-conversation, on-demand                         |
| Incremental updates      | No                        | Yes                                                 |
| Resumable transfers      | No                        | Yes (byte-range support)                            |
| Forward secrecy          | Yes (random key per sync) | Yes (KEK rotated on installation changes)           |
| Transfer size visibility | Unknown until complete    | Exact sizes known upfront for bandwidth planning    |
| Sync control             | Opaque background worker  | Explicit function calls with error handling         |

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

### Performance Characteristics

| Operation             | Time (3G) | Time (WiFi) | Bandwidth |
| --------------------- | --------- | ----------- | --------- |
| Initial sync (groups) | 1-3 sec   | < 500ms     | ~100 KB   |
| Load one conversation | 2-5 sec   | < 1 sec     | 100KB-2MB |
| Full background sync  | 1-10 min  | 10-60 sec   | 10-100 MB |
| Identity rotation     | 1-2 sec   | < 500ms     | ~10 KB    |
| Upload new messages   | 1-3 sec   | < 500ms     | ~100 KB   |

---

## Sync Identity

The MLS sync group is the secure channel through which installations share encryption keys and coordinate sync operations. It is separate from the sync server - the sync group handles key distribution while the server handles data storage.

### Distribution

When a new installation is created, it sends a sync identity request message to the MLS sync group. An existing installation acknowledges the request and generates a new sync identity with a rotated KEK for forward secrecy. All installations receive the new identity and store it in their local DB for use when syncing with the sync server.

```rust
/// Sync identity - distributed via MLS sync group, stored in local DB
struct SyncIdentity {
  /// Random 32-byte ID - to obscure inbox_id
  sync_id: String,
  /// Ed25519 keypair for authenticating with Web sync provider.
  /// Not used for iCloud/Google Cloud (platform handles auth).
  auth_keypair: Option<Ed25519Keypair>,
  /// Current Key Encryption Key for archives
  kek: [u8; 32],
  /// Creation timestamp
  created_at_ns: i64,
}

/// Request sync identity (sent by new installation)
struct SyncIdentityRequest {
  /// Random 32-byte ID to correlate request with acknowledgement
  request_id: String,
}

/// Acknowledge request (sent by leader before generating new identity)
struct SyncIdentityAcknowledge {
  /// Matches the request_id from SyncIdentityRequest
  request_id: String,
}
```

### Sync Identity Rotation

When an installation is created or revoked, the sync identity must be rotated to ensure forward secrecy - the revoked installation should not be able to decrypt future sync data. Once rotated, previous sync data will not be accessible either.

Rotation uses a two-phase approach to handle failures:

```rust
/// Phase 1: Claim rotation responsibility and share new identity
struct SyncIdentityRotationClaim {
  /// Random 32-byte ID to identify this rotation attempt
  rotation_id: String,
  /// The new sync identity (shared preemptively)
  new_identity: SyncIdentity,
}

/// Phase 2: Confirm rotation completed on server
struct SyncIdentityRotationConfirm {
  /// Matches the rotation_id from SyncIdentityRotationClaim
  rotation_id: String,
}
```

#### Flow

1. Installation created or revoked
2. New `SyncIdentity` generated
3. Broadcasts `SyncIdentityRotationClaim` with the new identity
4. All installations store the new identity (preemptively)
5. Claiming installation rotates keys on sync server
6. Broadcasts `SyncIdentityRotationConfirm` on success

#### Failure Handling

| Failure Point                     | State                                      | Recovery                                                                               |
| --------------------------------- | ------------------------------------------ | -------------------------------------------------------------------------------------- |
| Before claim sent                 | No change                                  | Another installation can claim                                                         |
| Claim sent, server rotation fails | All have new identity, server has old keys | Claimer retries server rotation with same identity                                     |
| Server rotated, confirm not sent  | All have new identity, server has new keys | System is functional; confirm is optional verification                                 |
| Claim sent, claimer goes offline  | All have new identity, server has old keys | After timeout, another installation retries server rotation using the claimed identity |

By broadcasting the new identity _before_ server rotation, we ensure all installations have the keys needed to decrypt regardless of where the process fails. The worst case is the server still has old keys, which can be retried. If a claim is received but no confirmation follows, installations can ignore the claim and another installation can attempt rotation.

**Note:** The content blobs are unaffected by this process. Only the manifest is updated with re-wrapped DEKs.

### Offline Installation Recovery

If an installation is offline during a KEK rotation, it recovers through the MLS sync group message history.

---

## Encryption Scheme

- Each archive has a unique random DEK
- DEKs are wrapped with KEK
- On rotation, all DEKs are re-wrapped with the new KEK
- Installations with old KEK cannot decrypt current archives

### Sync ID

Instead of using `inbox_id` (which links to on-chain identity), we generate a random `sync_id` that is:

- Unlinkable to XMTP identity
- Distributed only via MLS-encrypted sync group
- Rotated on every installation change

### Manifest

There is exactly one manifest per `inbox_id`, stored as `{sync_id}.manifest`. The manifest is the encrypted index that makes content blobs useful - without it, blobs are opaque and undecryptable since the manifest contains the wrapped DEKs needed to decrypt each blob.

The manifest will typically be in 2-10 KB range, but grows with number of groups/blobs.

```rust
/// Encrypted manifest - stored on server
struct EncryptedManifest {
  /// DEK wrapped with KEK
  wrapped_dek: Vec<u8>,
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
  /// Consent archives
  consent: Vec<ManifestEntry>,
  /// Group metadata archives
  groups: Vec<ManifestEntry>,
  /// Per-group message archives, keyed by group_id
  messages: HashMap<GroupId, Vec<ManifestEntry>>,
}

/// Metadata for an archive, stored in the manifest
struct ManifestEntry {
  /// SHA-256 hash of encrypted blob as hex string (used as filename on server)
  content_hash: String,
  /// DEK wrapped with KEK
  wrapped_dek: Vec<u8>,
  /// Random 12-byte nonce for AES-256-GCM decryption
  nonce: [u8; 12],
  /// Size of encrypted blob in bytes
  size_bytes: u64,
  /// When this entry was created
  created_at_ns: i64,
  /// Time range of data in this archive
  time_range_start_ns: i64,
  time_range_end_ns: i64,
  /// Number of items in this archive
  item_count: u64,
}

/// Encrypted archive stored on the sync server (raw AES-256-GCM ciphertext)
type EncryptedArchive = Vec<u8>;
```

### Key Wrapping

Each archive has a unique, random Data Encryption Key (DEK). The DEK is wrapped (encrypted) with the KEK.

```rust
/// Creates an encrypted archive from data.
///
/// Steps:
/// 1. Generate random 32-byte DEK
/// 2. Generate random 12-byte nonce
/// 3. Encrypt data with AES-256-GCM using DEK and nonce
/// 4. Wrap DEK with KEK using AES key wrap (RFC 3394)
/// 5. Return EncryptedArchive and ManifestEntry (metadata + wrapped DEK)
fn create_archive(data: &[u8], kek: &[u8]) -> (EncryptedArchive, ManifestEntry);

/// Decrypts an encrypted archive using wrapped DEK and KEK.
fn decrypt_archive(archive: &EncryptedArchive, wrapped_dek: &[u8], kek: &[u8]) -> Vec<u8>;
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
- Blobs are only useful with the corresponding manifest entry

---

## Client API

### Configuration

```rust
/// The cloud storage provider to use for device sync.
pub enum SyncProvider {
  /// Web-based storage with custom endpoint
  Web,
  /// Apple iCloud storage (iOS/macOS only)
  ICloud,
  /// Google Cloud storage (Android primarily)
  GoogleCloud
}

/// Configuration options for web-based sync storage.
pub struct WebSyncProviderOptions {
  /// The HTTP endpoint URL for managing sync data
  pub endpoint: String,
}

/// Configuration options for iCloud sync storage.
pub struct ICloudProviderOptions {}

/// Configuration options for Google Cloud sync storage.
pub struct GoogleCloudProviderOptions {}

/// Provider-specific configuration options for device sync.
/// Each variant contains the configuration for its respective cloud provider.
pub enum SyncProviderOptions {
  Web(WebSyncProviderOptions),
  ICloud(ICloudProviderOptions),
  GoogleCloud(GoogleCloudProviderOptions),
}


/// Client configuration for sync behavior
pub struct SyncClientConfig {
  /// When enabled, automatically upload local changes when a peer requests sync
  /// via the sync group. This ensures new installations get fresh data.
  /// Default: false
  pub auto_upload_on_request: bool,

  /// When enabled, periodically upload in the background at the specified interval (seconds).
  /// This keeps the server up-to-date for faster new device onboarding.
  /// Default: None (no automatic uploading)
  pub auto_upload_interval_secs: Option<u64>,

  /// Threshold in seconds for considering server data "stale". When downloading manifest,
  /// if last_updated_ns is older than this, the manifest is marked as stale.
  /// Default: None
  pub stale_threshold_secs: Option<u64>,

  /// Local filesystem path where downloaded sync data and manifests are cached.
  /// This directory is used to store temporary downloads before processing.
  /// Default: None (disable download resume)
  pub download_cache_path: Option<String>,

  /// The cloud storage provider to use for device sync (Web, iCloud, or GoogleCloud).
  /// If None, device sync functionality will be disabled.
  /// Default: None
  pub sync_provider: Option<SyncProvider>,

  /// Provider-specific configuration options for the selected sync provider.
  /// Must match the provider type specified in sync_provider.
  /// Default: None
  pub sync_provider_options: Option<SyncProviderOptions>,
}

impl Default for SyncClientConfig {
  fn default() -> Self {
    Self {
      auto_upload_on_request: false,
      auto_upload_interval_secs: None,
      stale_threshold_secs: None,
    }
  }
}
```

### Download Sync Data

Failed downloads can resume from the last successful byte position using HTTP Range requests. Downloads are cached every 256KB.

```rust
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

/// Result of a sync operation
pub struct SyncDownloadResult {
  /// Number of records imported
  pub records_imported: u64,
  /// Number of records skipped
  pub records_skipped: u64,
  /// Bytes downloaded
  pub bytes_downloaded: u64,
}

pub struct SyncDownloadResumeResult {
  /// Number of downloads successfully resumed and completed
  pub resumed: u64,
  /// Number of downloads that could not be resumed (will need to re-download)
  pub failed: u64,
  /// Total bytes downloaded during resume
  pub bytes_downloaded: u64,
}

/// Options for download operations
pub struct SyncDownloadOptions {
  /// Progress callback (bytes_transferred, total_bytes)
  pub on_progress: Option<Box<dyn Fn(u64, u64) + Send>>,
}

impl SyncClient {
  /// Download and decrypt manifest from sync server.
  /// Required for syncing consent, groups, and messages.
  pub async fn download_manifest(&self) -> Result<Manifest, SyncError>;

  /// Download and import consent records.
  pub async fn sync_consent(
    &self,
    manifest: &Manifest,
    opts: SyncDownloadOptions,
  ) -> Result<SyncDownloadResult, SyncError>;

  /// Download and import group metadata (names, settings, etc.).
  pub async fn sync_groups(
    &self,
    manifest: &Manifest,
    opts: SyncDownloadOptions,
  ) -> Result<SyncDownloadResult, SyncError>;

  /// Download and import messages for a specific group.
  pub async fn sync_group_messages(
    &self,
    manifest: &Manifest,
    group_id: &GroupId,
    opts: SyncDownloadOptions,
  ) -> Result<SyncDownloadResult, SyncError>;

  /// Download and import all message history for all groups.
  pub async fn sync_all_messages(
    &self,
    manifest: &Manifest,
    opts: SyncDownloadOptions,
  ) -> Result<SyncDownloadResult, SyncError>;

  /// Resume any downloads that were interrupted.
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
  /// 6. Delete partial file on success
  pub async fn resume_pending_downloads(&self) -> Result<SyncDownloadResumeResult, SyncError>;

  /// Calculate the exact download size for a sync operation without downloading.
  /// Use this to inform users of bandwidth requirements before starting.
  pub fn calculate_download_size(
    &self,
    manifest: &Manifest,
    scope: SyncScope,
  ) -> Result<SyncStorageUsage, SyncError>;
}
```

### Upload Sync Data

Failed uploads can be retried with automatic resume support. Pending upload data is stored in the local DB.

```rust
/// Pending upload tracked in local database for resume support
struct SyncUploadPending {
  /// SHA-256 hash of the complete blob (for integrity verification)
  content_hash: String,
  /// Total size of the blob in bytes
  total_size: u64,
  /// Encrypted blob
  encrypted_content: Vec<u8>,
  /// Timestamp when this upload was initiated (nanoseconds since epoch)
  started_at_ns: i64,
}

pub struct SyncUploadResult {
  /// Number of records uploaded
  pub records_uploaded: u64,
  /// Bytes uploaded
  pub bytes_uploaded: u64,
}

pub struct SyncUploadResumeResult {
  /// Number of uploads successfully resumed and completed
  pub resumed: u64,
  /// Number of uploads that could not be resumed (will need to re-upload)
  pub failed: u64,
  /// Total bytes uploaded during resume
  pub bytes_uploaded: u64,
}

impl SyncClient {
  /// Upload local consent records to sync server.
  pub async fn upload_consent(&self) -> Result<SyncUploadResult, SyncError>;

  /// Upload local group metadata to sync server.
  pub async fn upload_groups(&self) -> Result<SyncUploadResult, SyncError>;

  /// Upload messages for a specific group.
  pub async fn upload_group_messages(
    &self,
    group_id: &GroupId,
  ) -> Result<SyncUploadResult, SyncError>;

  /// Upload all group messages
  pub async fn upload_all_messages(&self) -> Result<SyncUploadResult, SyncError>;

  /// Resume any uploads that were interrupted (e.g., app killed, network lost).
  pub async fn resume_pending_uploads(&self) -> Result<SyncUploadResumeResult, SyncError>;

  /// Calculate the exact upload size for a sync operation without uploading.
  pub fn calculate_upload_size(&self, scope: SyncScope) -> Result<SyncStorageUsage, SyncError>;
}
```

### Manage Sync Data

```rust
/// Storage usage for a single group's messages
pub struct SyncStorageUsage {
  /// Total bytes for all archives
  pub total_bytes: u64,
  /// Number of archives
  pub archive_count: u64,
  /// Total item count across all archives
  pub item_count: u64,
  /// Time range covered by archives
  pub time_range_start_ns: i64,
  pub time_range_end_ns: i64,
}

/// Detailed breakdown of sync storage usage on the server
pub struct SyncStorageTotalUsage {
  /// Size of encrypted manifest
  pub manifest_bytes: u64,
  /// Consent storage usage
  pub consent: SyncStorageUsage,
  /// Groups storage usage
  pub groups: SyncStorageUsage,
  /// All messages storage usage
  pub messages: SyncStorageUsage,
  /// Per-group message storage usage
  pub messages_by_group: HashMap<GroupId, SyncStorageUsage>,
  /// Total storage across all categories
  pub total_bytes: u64,
}

impl SyncClient {
  /// Delete all sync data from the server (manifest and all blobs).
  /// This is a destructive operation - other installations will lose access to
  /// synced history until data is re-uploaded.
  /// Local data is NOT affected.
  pub async fn delete_all_server_data(&self) -> Result<(), SyncError>;

  /// Get detailed breakdown of sync storage usage on the server.
  /// Requires downloading the manifest to compute sizes.
  pub async fn storage_usage(&self) -> Result<SyncStorageTotalUsage, SyncError>;
}
```

### Sync Identity

```rust
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

impl SyncClient {
  /// Performs full identity rotation (triggered on installation add/revoke).
  ///
  /// Uses a two-phase approach to handle failures:
  ///
  /// Phase 1 - Claim:
  /// 1. Generate new SyncIdentity (new sync_id, auth keypair, KEK)
  /// 2. Broadcast SyncIdentityRotationClaim with new identity to sync group
  /// 3. All installations store the new identity preemptively
  ///
  /// Phase 2 - Server rotation:
  /// 4. Download and decrypt current manifest from server
  /// 5. Re-wrap all DEKs with new KEK
  /// 6. Create new manifest
  /// 7. Manage identity rotation on sync provider
  ///    - Deletes old manifest
  ///    - Stores new one
  ///    - Replaces auth public key
  /// 8. Broadcast SyncIdentityRotationConfirm on success
  /// 9. Update local sync_identity
  pub async fn rotate_identity(&self) -> Result<(), SyncError>;

  /// Get current sync status and statistics.
  pub fn status(&self) -> SyncStatus;
}
```

### Stale Data Handling

When server data is considered stale, new installations can request updates from peers via the MLS sync group.

```rust
/// Request peers to upload their latest data to the sync server
struct SyncDataRequest {
  /// Random 32-byte ID to correlate request with acknowledgement
  request_id: String,
}

/// Acknowledge sync data request (sent by installation handling the upload)
struct SyncDataAcknowledge {
  /// Matches the request_id from SyncDataRequest
  request_id: String,
}

/// Confirm that sync data has been updated (sent by installation handling the upload)
struct SyncDataComplete {
  /// Matches the request_id from SyncDataRequest
  request_id: String,
}

impl SyncClient {
  /// Returns true if the manifest is older than the given threshold.
  /// If threshold_secs is None, uses config.stale_threshold_secs.
  /// If both are None, returns false.
  pub fn is_stale(&self, threshold_secs: Option<u64>) -> bool;

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

---

## Server API (Web Provider)

This section describes the HTTP API for the Web sync provider. iCloud and Google Cloud providers use platform-native APIs with device-based authentication.

### Authentication

The Web provider uses Ed25519 signatures for request authentication. Each request includes headers:

- `X-Sync-Id`: The sync_id making the request
- `X-Signature`: Base64-encoded Ed25519 signature using the sync_id

The server:

1. Looks up the public key from `{sync_id}.key`
2. Verifies the signature using the sync_id

### Storage Model

```
{sync_id}.key       - Ed25519 public key (32 bytes)
{sync_id}.manifest  - Encrypted manifest
{content_hash}      - Encrypted content blobs (shared globally)
```

### Endpoints

All endpoints except `/register` require authentication headers: `X-Sync-Id`, `X-Signature`.

```
POST /register
  Body: { sync_id, auth_public_key }
  → Creates {sync_id}.key with auth_public_key
  → Returns error if sync_id already registered

GET /{sync_id}.manifest
  → Returns encrypted manifest

GET /{content_hash}
  Headers: Range (optional)
  → Returns blob by content hash
  → Supports Range requests for resumable downloads

PUT /{sync_id}.manifest
  Body: encrypted manifest blob
  → Stores/updates manifest

PUT /{content_hash}
  Body: encrypted blob
  → Stores blob (content-addressed, deduplicated)

POST /rotate
  Auth: signed with OLD key
  Body: RotateRequest
  → Deletes {old_sync_id}.key and {old_sync_id}.manifest
  → Creates {new_sync_id}.key and {new_sync_id}.manifest

DELETE /{sync_id}.manifest
  → Deletes manifest only (blobs are shared, not deleted)
```

### Rotation

```rust
/// Request to rotate sync identity on the server
struct RotateRequest {
  /// Old sync ID being replaced
  old_sync_id: String,
  /// New sync ID
  new_sync_id: String,
  /// New Ed25519 public key for authentication
  new_auth_public_key: [u8; 32],
  /// New encrypted manifest (re-wrapped DEKs with new KEK)
  new_manifest: EncryptedManifest,
}
```
