# Metadata catalogue fixture

This fixture runs the current backend binary with a published application
catalogue. It creates one database in the configured PostgreSQL service.
It uses the current S3 service. It does not change the shared backend catalogue.
The database, listeners and backend process belong to this run. The runner
removes them after success, command failure or cancellation. Backend logs remain
at the printed path.

Start PostgreSQL and S3 with the normal worktree recipes. Read the URLs from
`dev/docker/.env`. Build the current binary with
`dev/nix-shell 'just backend build'`. Use the resulting `result/bin/xmtp-backend`
path with `metadata_backend.py --backend PATH -- COMMAND`. Run the runner in a
Nix environment with Python 3.11 or later, `psql` and `grpc-health-probe`.
Export `DATABASE_URL`, `XMTP_S3_URL` and `XMTP_S3_BASE_URL` from that worktree.
The runner sets `XMTP_BACKEND_URL`, `XMTP_ANDROID_BACKEND_URL` and
`XMTP_METADATA_BACKEND_PORT` for `COMMAND`.

For Android instrumentation, run `COMMAND` in the existing owned emulator scope.
Reverse the fixture's `XMTP_METADATA_BACKEND_PORT` and the worktree S3 port with
`adb -s "$ANDROID_SERIAL" reverse`. Pass the fixture URL as the
`metadataBackendUrl` instrumentation argument. Select
`org.xmtp.android.example.messenger.metadata.MetadataEditorInstrumentedTest`.
Do not use the default backend URL for this test. The test requires all eight
published catalogue entries before it creates two clients and new conversations.

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
dev/nix-shell --shell android 'python3 -m unittest discover -s sdks/android/example/fixtures -p test_metadata_backend.py'
```

These checks use process stubs. They prove teardown and exit status behavior.
They do not prove metadata commits. The instrumented test uses two real clients
to check bytes, collection deltas, sibling entries, own fields in a group and DM,
empty and absent values, denied writes and stale save rejection.
