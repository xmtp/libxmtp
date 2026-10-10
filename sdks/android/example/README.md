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

Published history uses SDK pages of 50 raw rows. The SDK orders by sent time,
then local delivery sequence. The app preserves that order and uses opaque SDK
positions for older and newer queries. A short or empty converted list does not
prove completion. Raw continuation can advance past unreadable rows. A notice
shows conversion loss. Each operation uses at most four history reads; Load more
continues from the last consumed raw position.

The cache keeps three transcripts and at most 500 published rows per transcript.
Queued and failed messages have a separate 50-row recovery page. Use Older pending
messages and Newest pending messages to change this page. Refresh reads the selected
page again. Recovery uses sent time and an opaque message-ID boundary, so tied rows
remain reachable. Each recovery operation uses at most four SDK page reads. Raw
continuation can advance through an empty converted page. A conversion-loss notice
keeps the next pending page available.
Saved positions retain the message key, pixel offset and SDK tuple. Refresh
reads the window around that tuple. A deleted anchor still has a valid query
boundary. The app selects the next surviving newer row, then an older row, at
offset zero and shows Position changed. A rejected cursor clears the saved
position and opens the newest page. Published ties have no timestamp-bucket stop.

Unread counts use retained incoming Published text, Markdown, reply and remote attachment
rows with an insertion timestamp greater than the local read marker. Equal
timestamps count as read together. Imports can retain an older insertion time.
The count and marker reads are separate SDK calls.

Attachment, custom field and notification controls are enabled only when their
app feature wiring is present. Shared UI records contain no SDK or Android
objects.
