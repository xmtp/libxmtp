# Legacy migration fixtures

These files contain synthetic identities, keys, metadata, and messages. They contain no user data. Do not use their keys outside tests.

## Source and independence

The 64 SQL migrations are copied from libxmtp commit `6a8e969785c62e0923cf3d8531aff4eaac3c91b7` (Node SDK 6.1.0 and Browser SDK 7.1.0). `release-schemas.json` records each SQL hash and the checked release commits. iOS and Android 4.10.0 have the first 62 migrations. Their 4.11.0 releases have all 64. Agent SDK 2.3.0 uses Node SDK storage.

`producer.rs` was compiled with the legacy source at that commit. It creates a real OpenMLS group, adds a second member, reads metadata through the legacy getters, and compares those values with the persisted bincode group context. Its database is `metadata-seed.db3`; its independent expected values are `metadata-expected.json`. OpenMLS is pinned to `3fabbf23`; bincode is 1.3.3.

`appdata-producer.rs` was compiled at legacy commit `cc878025`. Its database is `appdata-seed.db3`. `appdata-context.bincode` is the stored `GroupContextGroupContext` record containing `AppData Group`. The fixture has valid name and description components and an invalid disappearing-message component. It checks that one invalid optional field does not remove independent fields.

The producers belong in `crates/xmtp_mls_common/examples/` in a checkout of their recorded source. They are fixture sources, not examples built by the current workspace. Run them with that checkout's Nix environment. MLS key generation is random, so a new seed has new IDs and bytes. Commit the seed, expected values, and hashes together.

To build either producer, add these workspace entries to the legacy package's
`[dev-dependencies]` if they are absent:

```toml
bincode.workspace = true
diesel.workspace = true
serde_json.workspace = true
openmls_basic_credential.workspace = true
openmls_rust_crypto.workspace = true
```

Copy the selected producer to `crates/xmtp_mls_common/examples/migration_fixture.rs`
in that legacy checkout. From the current repository's Nix environment, run:

```sh
dev/nix-shell 'dev/agent-run cargo run --manifest-path /path/to/legacy/Cargo.toml -p xmtp_mls_common --example migration_fixture -- /tmp/migration-fixture'
```

Copy the resulting `metadata.sqlite` and `normalized.json` as the seed and
expected values. For the AppData fixture, retain `metadata-full.sqlite` before
the producer removes secret rows, and extract the context that contains
`AppData Group`. Do not substitute a database made by the current SDK.

## Controlled history

Run from the repository root:

```sh
dev/nix-shell 'python3 crates/xmtp_legacy_migration/fixtures/generate.py'
```

The generator applies the copied legacy SQL and adds controlled records around the real MLS seed. It uses `protoc` to encode message content independently from the converter. It never calls the current SDK to create or migrate a source database.

| File | Schema | Expected groups/messages/consent |
| --- | --- | --- |
| `stable.db3` | All 64 migrations | 2 / 3 / 1 |
| `consent-states.db3` | Copy of `stable.db3`; all three consent states | 2 / 3 / 3 |
| `mobile-4.10.db3` | First 62 migrations | 2 / 3 / 1 |
| `early.db3` | First 36 migrations; no message expiry column | 2 / 5 / 1 |
| `encrypted.db3` and sidecars | All 64; SQLCipher; committed message in WAL | 2 / 4 / 1 |

`consent-states.db3` is a deterministic copy of `stable.db3` with only consent rows replaced. Its synthetic inboxes `02`, `03`, and `04` (each repeated 32 times) store Unknown, Allowed, and Denied as database values 0, 1, and 2. The corresponding archive values are 1, 2, and 3. `expected.json` records both sets. This derivative does not change the original producer fixtures or their hashes.

The encrypted fixture uses a 32-byte key of `0x11` and a 16-byte salt of `0x22`. Its plaintext header is 32 bytes. The WAL stays separate because the producer process exits without closing the connection.

The fixtures contain an ordinary group, a DM with legacy metadata, excluded sync and one-shot groups, and a restored group. Messages include permanent history, past and future stored deadlines, a non-application message, and nanosecond values above JavaScript's safe integer range. `early.db3` retains all eligible application history because the source has no recorded message deadlines. Legacy migration timestamps do not control eligibility.

`hashes.json` records the database, WAL, salt, and bincode hashes. Native tests copy the files and compare source bytes before and after success and failure. Browser tests import the SQLite fixture into the real legacy OPFS pool and compare exports before and after conversion.
