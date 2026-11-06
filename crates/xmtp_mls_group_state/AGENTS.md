# xmtp_mls_group_state

Group state the client and the backend both read: the app data dictionary
and its components, group metadata, inbox IDs and versions; no client DB or
Docker. Native and WASM. Run
`dev/nix-shell 'just test crate xmtp_mls_group_state'`.

- Group roots and legacy commit-log signers are secrets; never log group contexts.
- Change metadata attributes through `insert_attribute` / `remove_attribute`
  so replaced signer secrets are wiped. Hold signer bytes as `Secret` or
  `Zeroizing` until OpenMLS owns them. Zeroization proofs: `--test zeroize`,
  which reads the group-key vectors from `xmtp_mls_group_keys`.
