# Metadata catalogue fixture

This fixture runs the current backend binary with a published application
catalogue. It creates one database in the configured PostgreSQL service.
It uses the current S3 service. It does not change the shared backend catalogue.
The database, listeners and backend process belong to this run. The runner
removes them after success, command failure or cancellation. Backend logs remain
at the printed path.

An internal keeper retains the process-group identity after the command exits.
Command status is reported separately; no poll or wait reaps the keeper before
the final group signal. Cleanup sends TERM, waits for the command, then sends
KILL and reaps the keeper. Owner-channel closure also stops its owned group.
Cleanup never uses a process-table scan or a global process kill.

Run from the repository root:

```sh
dev/nix-shell 'just backend up'
dev/nix-shell 'just android metadata-fixture-test'
dev/nix-shell 'just android metadata-fixture-smoke'
dev/nix-shell 'just android example-test-integration'
```

The integration recipe stages the current backend and Android SDK. The Android
Nix shell supplies the pinned PostgreSQL client and `grpc-health-probe`. The
runner creates a catalogue backend and starts the owned emulator scope. The
recipe forwards its listener and the worktree S3 port. It passes the catalogue
URL as the `metadataBackendUrl` instrumentation argument. The runner keeps the
normal backend URLs for the other app tests. No shared database or catalogue is
changed. Set `XMTP_METADATA_LOG_DIR` to keep logs at a selected path.

The child command receives `DATABASE_URL`, `XMTP_DATABASE_URL` and
`XMTP_REPLICA_URL` for the owned database. The backend and S3 HTTP endpoints
stay available for the app tests. These environment values are not a sandbox.
Decoded `dbname` query parameters are removed from the owned URL so they cannot
override the UUID database path. Other connection options stay intact.

After backend health succeeds, the child receives `XMTP_METADATA_BACKEND_LEASE`.
It names a mode 0600 JSON file in the mode 0700 fixture directory. Its
`formatVersion`, `url`, `port`, `database`, `serverPid`, `groupPid`, `ownerPid`
and `uid` fields identify the ready disposable target without credentials.
`groupPid` is the retained keeper/session; `serverPid` is the backend command.
The descriptor is removed when the fixture scope exits. The performance runner
uses this record to admit only the owned target.

Every PostgreSQL command removes the URI password from its arguments. If the
URI has a password, the command reads it from an owned `PGPASSFILE` with mode
0600 in a mode 0700 directory. The runner removes that file after cleanup.
Other connection settings stay the same. This uses the
[PostgreSQL password file](https://www.postgresql.org/docs/18/libpq-pgpass.html).

Select the metadata test with:

```sh
dev/nix-shell 'just android example-test-integration -Pandroid.testInstrumentationRunnerArguments.class=org.xmtp.android.example.messenger.metadata.MetadataEditorInstrumentedTest'
```

The test requires all twelve published catalogue entries before it creates new
conversations. Each run uses two real clients. An existing conversation can
lack a registered field; this state is checked in the app host tests.

`metadata-catalogue.toml` is also a complete catalogue example for an operator.
Append its `[[application_components]]` entries to a backend configuration.
Keep the IDs, names, types and policies stable. Restart that backend. Create a
new group or DM to register eligible fields. The admin field is group only.
An old conversation can lack a new field. The app reports this state and does
not make a separate registry commit.

The group fields are String, Bytes, a Bytes-keyed Bytes map, Bytes and InboxId
sets, and an admin field. `USER_EXAMPLE_NOTE` and `USER_EXAMPLE_KEY` are
self-owned InboxId maps. My fields changes only the caller's entries in the
current conversation. Empty values and absent values are different.

Run local ownership checks with:

```sh
dev/nix-shell 'just android metadata-fixture-test'
```

These checks use process stubs. They prove teardown and exit status behavior.
They do not prove metadata commits. The instrumented test uses two real clients
to check bytes, collection deltas, sibling entries, own fields in a group and DM,
empty and absent values, denied writes and stale save rejection.

The original eight definitions remain stable. Four immutable definitions use
IDs 0xFD00–0xFD03 for a group String, Bytes map, Bytes set and own String map.
An immutable component permits one initial value. Each later write or removal
is forbidden, even when a different member has no own entry in that map.
The app shows the committed value and disables later controls.

The runner defers signals through CREATE completion and verifies the exact
allocated database on uncertain command failure. Cleanup ignores further
SIGINT/SIGTERM until its owned processes and database are removed. It never
claims an already existing allocated name. Local signal tests send real signals
during the CREATE process.
