# XMTP Messenger

This is an Android example for the current `uniffi.xmtp_sdk` package. The Android
host owns the client and device services. Compose screens and app records are in
`example-shared/src/commonMain`.

This app is for testing and debugging purposes only.

## Build and check

Run these commands from the repository root:

```sh
dev/nix-shell 'just android assemble'
dev/nix-shell 'just android example-check'
dev/nix-shell 'just android example-test'
dev/nix-shell 'just backend up'
dev/nix-shell 'just backend status'
dev/nix-shell 'just android example-test-integration'
```

The integration recipe owns its emulator and stops it after the tests. It uses
the current worktree backend and object-store ports. SDK package, consumer and
process lifecycle checks remain separate gates.

## Connect

The Start screen shows the backend URL from the worktree build configuration.
You can edit it. Use HTTPS for a remote backend. HTTP is allowed only for the
local development hosts `localhost`, `127.0.0.1`, `[::1]` and the Android emulator
host `10.0.2.2` in debug builds. Release builds permit HTTP only for the listed
loopback hosts. Do not put credentials in the URL.

The Credential field appears only when the selected server reports that it
requires authentication. Enter that server's credential in this field. If a
connection fails, Retry uses the current URL and credential. A failed saved
connection keeps its saved credential when the field is empty.

The app retains one hidden wallet per backend profile. Private key, credential
and database key records use Android Keystore encryption. App and SDK files are
excluded from backup.

An old example database with no retained wallet key shows a migration state.
Select its inbox only when you want to reset that local account. First launch
does not remove an old database or select another inbox's database.

Sign out stops the client and clears its credential. It keeps the wallet and SDK
files. Delete my account removes the selected profile's local database, SDK
attachment files, temporary sources, exported files and app records. This is a
local device reset. A failed reset retains its cleanup record for Retry.

## Chat

Allowed and Unknown tabs use current consent. An Unknown chat has Allow and
Block actions. New conversations accept lowercase inbox IDs or Ethereum
addresses. Groups offer All members and Admins only presets.

Messages support text, replies, emoji reactions and remote deletion. “Delivered”
means that the SDK reports Published. It does not mean that a recipient read the
message. The composer keeps text until the SDK accepts a message ID. A definite
failure before acceptance keeps the draft. New edits stay in the composer when
an earlier send completes. Publication Retry names the accepted message ID. A send interrupted
before its ID is saved has an unknown outcome. Settings offers View chat and
Discard record. It does not resend that record automatically.

Group settings use current names, descriptions, roles, built-in permissions and
disappearing settings. A preset change uses separate SDK writes. After the first
write starts, navigation does not stop the remaining writes to that group while
the same session is active. A partial SDK failure shows the state read from the
SDK. Request removal shows PendingRemove
until a later committed state changes it.

## Local history limits

The first page loads 50 rows, with complete timestamp buckets when a boundary
has ties. Raw matching counts prove coverage. An unconvertible stored row can
shorten the returned list; a short list alone does not prove completion. The app
stops visibly when the current SDK cannot cover a bucket within its 501-row query
limit. It never advances past an unretained member of that bucket.

The cache keeps three transcripts and at most 500 published rows per transcript.
Queued and failed messages have a separate 50-row recovery page. Use Older pending
messages and Newest pending messages to change this page. Refresh reads the selected
page again. A visible notice stops a timestamp tie that exceeds 50 rows. Retained positions
restore the message key and pixel offset. After eviction, the app makes one
bounded timestamp query. If the key is absent, it selects a surviving row at
offset zero and shows Position changed.

Unread counts use retained incoming Published text, Markdown, reply and remote attachment
rows with an insertion timestamp greater than the local read marker. Equal
timestamps count as read together. Imports can retain an older insertion time.
The count and marker reads are separate SDK calls.

Attachment, custom field and notification controls are enabled only when their
app feature wiring is present. Shared UI records contain no SDK or Android
objects.
