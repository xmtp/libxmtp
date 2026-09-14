# Types, configuration, cryptography, identity

## Newtypes, not `Vec<u8>` or `String`

All in `xmtp_proto::types`. Infallible `From` for fixed-size arrays, `TryFrom`
with `ConversionError` for slices and vectors.

```rust
use xmtp_proto::types::{Cursor, GroupId, InstallationId, Topic, TopicCursor};

let group_id = GroupId::try_from(raw.as_slice())?;     // [u8; 16]; From<[u8; 16]>, FromStr (hex)
group_id.as_slice(); group_id.to_vec(); group_id.to_openmls();
let installation: InstallationId = (*key.public_bytes()).into();  // [u8; 32]; to_vec, Deref, TryFrom
let topic = Topic::new_group_message(group_id);        // never concatenate topic bytes by hand
let topic = Topic::parse(&wire.topic)?;                // validates the kind byte and id length
topic.kind(); topic.identifier();
let cursors: TopicCursor = HashMap::from([(topic, Cursor(0))]);   // Cursor(u64); 0 = from the start
```

Other constructors: `Topic::new_welcome_message(installation_id)`,
`new_identity_update(inbox_id_bytes)`, `new_key_package(installation_id)`,
`new_commit_log(group_id)`. `GroupId` and `ConversationType` have Diesel impls
behind the `diesel` feature. `GroupMessage` and `WelcomeMessage` are the decoded
payload wrappers; `crates/xmtp_api_backend/src/envelope.rs` builds them from
wire envelopes.

Inbox ids are lowercase hex `String` (`xmtp_common::types::InboxId`).
Normalize untrusted input:

```rust
use xmtp_common::hex::NormalizeHex;
let inbox_id = input.normalize_hex();    // lowercases, strips 0x. Never hand-roll this.
```

## Configuration

`xmtp_configuration` holds every value shared by more than one crate. A
constant used by one module stays in that module. Every number has a name.

- `common/{api,backend,db,metadata,mls,scw,streams,tracing}.rs`: one value for
  every build. Add a file plus a `mod` and `pub use` line for a new area.
- `prod/` and `test/`: same symbol names, different values (`MAX_PAGE_SIZE` is
  100 and 20). `test/` replaces `prod/` under `cfg(any(test, feature = "test-utils"))`.
- A value that differs per worktree is a function, not a constant:
  `backend_test_url()`, `backend_test_toxic_url()`, `DockerUrls::anvil()` read
  the environment and fall back to a `*_DEFAULT`. Wasm bakes the value in at
  build time with `option_env!`.

```rust
use xmtp_configuration::BACKEND_DEFAULT_MAX_QUERY_LIMIT;
self.query_all(cursors, BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32)
```

## Cryptography

`xmtp_cryptography` owns every primitive. `xmtp_common` re-exports hashing and
randomness at its root.

```rust
xmtp_common::sha256_array(&data);     // [u8; 32], allocation-free
xmtp_common::sha256_bytes(&data);     // Vec<u8>
xmtp_common::rng(); xmtp_common::rand_array::<32>(); xmtp_common::rand_secret::<32>();
```

Native entry points call `xmtp_cryptography::install_crypto_provider()` before
building any TLS client. It is idempotent. The `ctor` fallback does not run
inside an Apple static library, so the bindings call it explicitly
(`bindings/node/src/client/create_client.rs`, `bindings/mobile/src/mls.rs`).

Installation keys: `XmtpInstallationCredential` with `CredentialSign` and
`CredentialVerify`. Test wallet: `generate_local_wallet()`.

## Identity

`xmtp_id` owns inbox ids, associations, signature requests, and smart-contract
wallet checks. Validation rules are in `crates/xmtp_id/AGENTS.md`.

```rust
let ident = Identifier::eth(address)?;                // sanitizes; also passkey(..), from_proto(..)
let inbox_id = ident.inbox_id(nonce)?;                // the only derivation; never re-implement the hash
let mut request = SignatureRequestBuilder::new(inbox_id)
    .create_inbox(ident.clone(), nonce)
    .add_association(new_member, existing_member)
    .build();
request.add_signature(sig, &scw_verifier).await?;
let update = request.build_identity_update()?;
```

Take `impl SmartContractSignatureVerifier`, never a concrete verifier.

## Message ids

`xmtp_mls::utils::id::calculate_message_id(group_id, payload, idempotency_key)`
is the only derivation. If the backend needs it without `xmtp_mls`, move it to a
shared crate. Client MLS message ids and envelope hashes stay separate rules.

## Shared MLS types

`xmtp_mls_common` holds types both the backend and the client read: group
metadata, mutable metadata, app data, commit-log decoding, invites. Put a type
there, not in `xmtp_mls`, when the backend must read it.
