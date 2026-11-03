# xmtp_mls_group_keys

Keys derived from the MLS group root (`group_keys`), commit-log sealing over
them (`commit_log`) and welcome payload encryption (`mls_ext`); no client DB
or Docker. Native and WASM. Run
`dev/nix-shell 'just test crate xmtp_mls_group_keys'`.

- Seal/open commit logs through `commit_log::{seal_entry, open_entry}`, over
  `group_keys`. Unsealed entries are retired; `ClientEnvelope` field 5 is reserved.
  Before changing group-key derivation or signing, read [group keys](src/group_keys.rs).
- Group roots and legacy commit-log signers are secrets; never log group contexts.
- `xmtp_mls_group_state`'s zeroize canaries also read
  `src/group_keys/fixtures/vectors.txt`.
