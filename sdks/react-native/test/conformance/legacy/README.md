# React Native 5.7.0 migration source

This fixture contains synthetic test history and keys. It was produced by the
public `xmtpv3` native API from the exact libxmtp source embedded in
`@xmtp/react-native-sdk` 5.7.0. The producer ran on macOS arm64. The old RN
JavaScript runtime was not executed. The RN 8 installed-package conformance
journey owns conversion and normal archive import on iOS and Android.

`source-provenance.json` pins the old npm package, its RN revision, both embedded
native revisions, all 62 SQL migration hashes, the producer inputs, and the
actual backend images. The Android and iOS native revisions differ only in
`Package.swift`; their `bindings/mobile`, `crates`, and `Cargo.lock` Git objects
are identical. The fixture uses the Android revision of that shared code.

## Independent records

`producer.rs` creates a new encrypted native client, signs and registers its
synthetic identity on a dedicated local legacy backend, creates one group,
sends two text messages, and stores denied consent for a synthetic inbox. The
SDK also stores allowed consent for the created group. Device sync is disabled.
The producer reads message IDs, content, timestamps, group metadata, and consent
through the old public API. Those readings and the known input text form
`expected-records.json`; the converter does not generate expectations.

Expected counts are one group, two messages, and two consent records. The
32-byte source database key is `0x53` repeated 32 times. The public synthetic
wallet key is `0x37` repeated 32 times; the producer accepts only a loopback
backend. None of these keys is suitable for other use.

The producer drops its group and conversation owners, calls
`release_db_connection`, drops the client, and exits successfully before the
fixture files are copied. The closed source consists of
`react-native-5.7.0.db3` and its `.sqlcipher_salt` file. WAL, SHM, and rollback
journal files are absent after the clean close. Preserve their absence as well
as the recorded hashes. The database contains synthetic legacy installation
state because it is an actual SDK database. Conversion must export only the
normal archive history and consent records.

## Reproduce the producer

Run these commands from a current libxmtp checkout. Start with absent temporary
source, producer, and output directories. `producer-Cargo.toml` intentionally
records the actual absolute source paths used for this one-off host producer.

```sh
mkdir -p /private/tmp/rn-legacy-native-source /private/tmp/rn-legacy-producer/src
git archive da3a7eff05445b0dde5a4ea4f68ec5e9a22bab27 | tar -x -C /private/tmp/rn-legacy-native-source
cp sdks/react-native/test/conformance/legacy/producer.rs /private/tmp/rn-legacy-producer/src/main.rs
cp sdks/react-native/test/conformance/legacy/producer-Cargo.toml /private/tmp/rn-legacy-producer/Cargo.toml
cp sdks/react-native/test/conformance/legacy/producer-Cargo.lock /private/tmp/rn-legacy-producer/Cargo.lock
docker compose -p xmtp-rn-legacy-fixture-f0a3d5e -f sdks/react-native/test/conformance/legacy/legacy-backend.compose.yml up -d --wait
docker compose -p xmtp-rn-legacy-fixture-f0a3d5e -f sdks/react-native/test/conformance/legacy/legacy-backend.compose.yml port node 5556
```

The last command reports the assigned loopback port. Substitute that port below.
The captured run used port 32797 and exited with code 0 on 2026-10-16.

```sh
dev/nix-shell 'RUSTC_WRAPPER= RUSTFLAGS="--cfg tracing_unstable -A ambiguous-glob-imports" CARGO_TARGET_DIR=/private/tmp/rn-legacy-target CARGO_BUILD_JOBS=4 XMTP_RTK=0 dev/agent-run cargo run --locked --manifest-path /private/tmp/rn-legacy-producer/Cargo.toml -- /private/tmp/rn-legacy-produced-valid http://127.0.0.1:32797'
```

The standalone manifest repeats the historical workspace's Diesel patches.
Every dependency name/version/source identity in its lock is present in the
original native lock; only the producer package is added. The build uses the
current pinned Rust 1.97.1 and allows the old Diesel fork's
`ambiguous-glob-imports` compatibility lint. It changes no historical native
source. SQLCipher 4.6.1 is bundled by the historical `libsqlite3-sys` 0.35.0.
The optional compiler cache is disabled only for this external producer.

MLS randomness and timestamps change when regenerating. Keep the resulting
database, salt, independent expected records, and new hashes together.

## Inspection and source preservation

The captured inspection copied the database and every present sidecar to a
temporary directory before opening SQLite. On that copy, it applied the fixture
key, `cipher_plaintext_header_size=32`, and the salt from the copied sidecar,
then enabled `query_only`. `source-inspection.json` records its exact query,
observed counts, all migration versions, and original hashes before and after.
SQLCipher 4.17.0 reported `integrity_check = ok`. The 62 versions exactly equal
the converter's first 62 accepted versions. Source hashes stayed unchanged.
Never open the committed source through SQLite during inspection.

The current self-hosted backend does not serve the old
`/xmtp.identity.api.v1.IdentityApi/GetInboxIds` RPC. The captured producer uses
its dedicated, digest-pinned legacy backend. The failed attempt against the
current backend is excluded from this fixture. Archive conversion and import
need no legacy backend.
