# 003: Message Security

Status: backend admission and identity trust limits approved on 2026-09-04. The full client MLS security description is completed during Phase 3.

This spec defines the security boundary of the self-hosted backend. It preserves existing validation behavior. It does not add validation rules or claim that all validly stored data is safe for a client to apply. Requirements use `SEC-nnn`.

## 1. Trust boundary

- SEC-001: The backend returns unsigned envelopes over trusted transport. Public endpoints use HTTPS. Protect the plaintext backend port from untrusted access. Phase 6 caller authentication does not authenticate group-message membership.
- SEC-002: Group keys and MLS group state remain on clients. A stored group message is not proof that its publisher belongs to the group. Clients authenticate and process MLS messages before applying their content.
- SEC-003: Topic derivation binds routing to a payload's identifier. It does not authorize the publisher. A caller can submit parseable data that names a known group. Such data can consume storage and client processing capacity without revealing group plaintext.
- SEC-004: Backend retention and client disappearing messages are separate. The expiry timestamp is not a message-deletion command for a client. Commit/proposal retention exemptions are based on parsed content type, not verified membership.
- SEC-005: Before caller authentication and quotas are added, deploy the backend on a trusted or externally restricted network. CORS is browser policy, not caller authentication. Permanent commit and commit-log retention has a spam and storage-abuse cost that later quotas must address.

## 2. Preserved payload validation

| Kind | Required behavior | Not established by acceptance |
| --- | --- | --- |
| Group message | Parse an MLS protocol message; derive the 16-byte group ID; mark commits and proposals | Membership, sender signature, epoch continuity, or decryptability |
| Key package | Exact TLS decode and existing cryptographic key-package validation; derive the installation key | Inbox registration, current installation membership, or additional XMTP policy checks |
| Identity update | Verify signatures and apply the association state machine against one complete history snapshot | Caller authorization to use the backend itself |
| Welcome | Parse the selected version and derive its 32-byte destination | Destination registration or decryptability |
| Commit-log entry | Decode the plaintext entry and its 16-byte group ID; retain its signature | Signature validity, epoch continuity, or hash-chain validity |

- SEC-010: Group-message parsing accepts trailing bytes, as before. Preserve the full accepted payload bytes on storage and return. Do not add an exact-consumption check or ciphersuite allow-list during extraction.
- SEC-011: The group commit/proposal flag is true for both MLS commits and proposals. It is false for other envelope kinds. It must not be used as proof of sender authorization.
- SEC-012: Preserve key-package verification of leaf and outer signatures, the MLS version, key separation, supported key-package extensions, and the current lifetime check. Preserve the conversion to a basic credential and the decoded credential value.
- SEC-013: Do not add key-package credential inbox-ID format checks, an XMTP ciphersuite allow-list, extra leaf-local capability checks, or a maximum lifetime-range check. The existing path does not perform them. A credential inbox ID can therefore be empty or malformed even when the package passes its existing checks. Topic derivation must still satisfy the installation-key length in spec 001.
- SEC-014: Welcome destinations need not be registered installations. Welcome pointers use random destinations. Both inline and pointer forms remain supported. Structural parsing is not decryption or signature validation.
- SEC-015: Commit-log signatures are returned without verification by the backend. The client checks epoch and hash-chain continuity. Never drop or reorder committed log entries: skipping one defeats the client's fork detection.
- SEC-016: The canonical envelope hash covers the protobuf envelope encoding. Inner byte fields are preserved exactly. This hash is distinct from the client's MLS message ID and payload hash. Share the canonical envelope encoder across backend and clients.

## 3. Identity state and identifiers

- SEC-020: Signature verification precedes application of the association state machine. Preserve existing inbox derivation, replay protection, recovery authorization, association changes, and chain-ID checks. The state machine receives verified updates.
- SEC-021: Read the complete prior log in one snapshot and record the greatest sequence ID actually read. Before committing a new update, compare it with the current inbox head under the identity lock. A change returns `ABORTED`; no new row is stored.
- SEC-022: Historical signature verification can require chain RPC. Replaying stored history is not necessarily CPU-only. A transient verifier error remains retryable and must not be reclassified as an invalid identity update.
- SEC-023: Normalize identifier lookup keys and verified projection keys by kind. Preserve original request values in positional lookup responses. Do not normalize signed fields before verification or change the transcript that was signed.
- SEC-024: Preserve the existing recovery-identifier behavior. A malformed or mixed-case Ethereum recovery identifier can be stored where the existing state machine accepts it. A later canonical signer may then fail to match that value, making recovery unusable. This is a known retained limit, not a new rejection rule.
- SEC-025: Identifier resolution is scoped by identifier kind. The greatest non-revoked association sequence ID wins. Revoking the latest active association can expose an older active association to a different inbox. Installation keys are not in this lookup projection.
- SEC-026: The identity-entry cap applies to new writes only. An exact duplicate still succeeds. Existing histories longer than the cap remain readable and usable for validation; a new update cannot extend them.

## 4. Passkey signatures

- SEC-030: Preserve verification of the challenge-bound P-256 signature over authenticator data and the client-data hash. Preserve current signature parsing and canonical replay-key encoding.
- SEC-031: Passkey identity equality is based on the public key. The reported relying party does not form part of identity equality. Do not change this rule during backend integration.
- SEC-032: The existing verifier does not validate ceremony type, expected origin, RP ID hash, user-presence or user-verification flags, signature counters, or a minimum authenticator-data length. Preserve these limits. Do not describe this path as full WebAuthn relying-party verification.
- SEC-033: A signature still must bind the expected identity-update challenge and verify with the claimed key. The missing WebAuthn checks are not evidence that an attacker can forge that signature. Adding an RP registry, changing origin trust, or adding validation checks requires separate approval.

## 5. Smart-contract-wallet verification

- SEC-040: Verify supported SCW signatures using operator-configured chain RPC routes. Requests select a chain/account, not an arbitrary RPC URL. A request with no block number asks for latest state; a supplied number asks for that block. Successful responses report the resolved block number.
- SEC-041: Preserve existing verifier errors and retryability. Provider and I/O failures are retryable. A missing configured verifier is also retryable and returns `UNAVAILABLE`, even if the configuration will remain missing until an operator changes it. Invalid cryptographic signatures remain terminal validation failures.
- SEC-042: Keep the bounded per-instance SCW verdict cache, default 10,000 entries with LRU eviction. Its key includes chain ID, account address, message hash, signature bytes, and the supplied block number with an explicit presence tag. Use an unambiguous encoding. Never key only by identifier or signature.
- SEC-043: Cache positive and negative verdicts for requests with an explicit block number. Do not cache verifier errors. Requests without a block number bypass cached verdicts and are not stored under a permanent “latest” key. This closes the previously identified stale latest-state result while retaining the cache for repeated historical verification.
- SEC-044: Preserve the existing numbered-block cache behavior: entries remain until LRU eviction or process restart. There is no automatic reorganization invalidation. A reorganization at a cached block number can leave a stale verdict until eviction. Document this limit; do not claim immediate revocation or reorganization correctness from LRU caching.
- SEC-045: Historical signature verification can require archive-capable RPC providers. An unavailable historical block is a verifier failure, not proof of an invalid user signature. Cache loss changes cost, not the stored identity history.

## 6. Errors and evidence

- SEC-050: Return typed validation failures under the reason codes in spec 001. Preserve sub-error information internally. Do not classify a failure by matching error-message text.
- SEC-051: Logs must not contain raw payloads, full installation identifiers, private keys, database credentials, or resolved RPC secrets. Use the shared logging and identifier-formatting conventions.
- SEC-052: Integration tests must pin both accepted and rejected edge cases above. Do not turn a retained limitation into an extra rejection while moving the validator. Test cache hits separately from latest-state bypass and transient errors.

## Review record

[Approved architecture review](https://plan.ref.tools/xWi9jEu8VHmuLI0W), 2026-09-04: preserve existing validation behavior, clarify edge cases, and keep the verifier cache. The latest-state cache correction was already part of the original architecture draft. Broader validation hardening is not approved by this document.
