# Metadata catalogue fixture

This fixture runs the current backend binary with a published application
catalogue. It creates one database in the configured PostgreSQL service.
It uses the current S3 service. It does not change the shared backend catalogue.
The database, listeners and backend process belong to this run. The runner
removes them after success, command failure or cancellation. Backend logs remain
at the printed path.

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

Select the metadata test with:

```sh
dev/nix-shell 'just android example-test-integration -Pandroid.testInstrumentationRunnerArguments.class=org.xmtp.android.example.messenger.metadata.MetadataEditorInstrumentedTest'
```

The test requires all eight published catalogue entries before it creates new
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
