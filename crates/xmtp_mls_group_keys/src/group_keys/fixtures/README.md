# Group-key test vectors

These are public test values. Do not use them outside tests.

`vectors.txt` pins, byte for byte, what `group_keys` must produce from one
group root:

- the HKDF-SHA256 pseudorandom key and the four keys of the group key table;
- the group id and the commit-log id;
- one sealed commit-log entry: its plaintext (a protobuf
  `PlaintextCommitLogEntry`), topic, nonce, ciphertext, `CommitLogEntryTBS`,
  and signature;
- one group message's `GroupMessageTBS` and signature.

`generate_vectors.py` computes them without libxmtp. It uses only the spec's
salt, labels, and structures, with HKDF, HMAC, Ed25519, and ChaCha20-Poly1305
from pyca/cryptography. It writes the TLS variable-length vectors and the
protobuf encoding out by hand. Never regenerate the vectors from
`GroupKeys::derive` or any other libxmtp code: they would then test the code
against itself.

## Regenerate

Run from the repository root. The command uses Python and `cryptography` from
the nixpkgs revision that `flake.lock` pins (`nixpkgs_2`). The vectors were
generated with Python 3.14.6 and `cryptography` 49.0.0.

```sh
nix shell --impure --expr '(builtins.getFlake "github:NixOS/nixpkgs/624af665418d3c65d544145b4d34ad696439570e").legacyPackages.${builtins.currentSystem}.python3.withPackages (p: [ p.cryptography ])' \
  -c python3 -I crates/xmtp_mls_common/src/group_keys/fixtures/generate_vectors.py \
  > crates/xmtp_mls_common/src/group_keys/fixtures/vectors.txt
```

The output is deterministic: Ed25519 signing, the derived nonce, and the AEAD
take no randomness. A rerun must leave `vectors.txt` unchanged.

## Check

```sh
dev/nix-shell 'just test workspace -p xmtp_mls_common group_keys::'
```
