# xmtp_mls_common

Shared MLS types for client/backend; no client DB or Docker. All modules support
native and WASM. Run `dev/nix-shell 'just test crate xmtp_mls_common'`.

- Seal/open commit logs through `commit_log::{seal_entry, open_entry}`, over
  `group_keys`. Unsealed entries are retired; `ClientEnvelope` field 5 is reserved.
  Before changing group-key derivation or signing, read [group keys](src/group_keys.rs).
- Group roots and legacy commit-log signers are secrets; never log group contexts.
- Change metadata attributes through `insert_attribute` / `remove_attribute`
  so replaced signer secrets are wiped. Hold signer bytes as `Secret` or
  `Zeroizing` until OpenMLS owns them. Zeroization proofs: `--test zeroize`.
