  ---
  Context Summary

  Task: Implement https://github.com/xmtp/libxmtp/issues/2691 — define "Unpacked" protobuf variants in xmtp_proto to eliminate redundant nested prost::Message::decode() calls when traversing OriginatorEnvelope → UnsignedOriginatorEnvelope → PayerEnvelope → ClientEnvelope.

  Status: All code changes are complete. Need to: verify compilation, run tests, commit, and create PR.

  ---
  What Was Done

  Core Idea

  Protobuf wire type 2 (length-delimited) is identical for both bytes and message fields. By defining new "Unpacked" structs where nested Vec<u8> fields are replaced with Option<MessageType> at the same tag numbers, prost automatically decodes all nesting in a single pass instead of requiring 3 sequential decode() calls.

  Files Created

  - crates/xmtp_proto/src/types/unpacked_envelopes.rs — New file defining:
    - UnpackedPayerEnvelope (tag 1: Option<ClientEnvelope> instead of Vec<u8>)
    - UnpackedUnsignedOriginatorEnvelope (tag 4: Option<UnpackedPayerEnvelope> instead of Vec<u8>)
    - UnpackedOriginatorEnvelope (tag 1: Option<UnpackedUnsignedOriginatorEnvelope> instead of Vec<u8>)
    - UnpackedQueryEnvelopesResponse (same wire format as QueryEnvelopesResponse but with Vec<UnpackedOriginatorEnvelope>)
    - UnpackedSubscribeEnvelopesResponse (same wire format as SubscribeEnvelopesResponse but unpacked)
    - impl TryFrom<&OriginatorEnvelope> for UnpackedOriginatorEnvelope (encode-then-decode for boundary conversions)

  Files Modified

  ┌─────────────────────────────────────────────────────────────────────────────┬───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
  │                                    File                                     │                                                                                            Change                                                                                             │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_proto/src/types.rs                                              │ Added mod unpacked_envelopes; pub use unpacked_envelopes::*;                                                                                                                                  │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_proto/src/api_client/impls.rs                                   │ Added Paged impls for both UnpackedQueryEnvelopesResponse and UnpackedSubscribeEnvelopesResponse                                                                                              │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/protocol/traits/visitor.rs                         │ Changed visit_originator, visit_unsigned_originator, visit_payer to accept unpacked types                                                                                                     │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/protocol/impls/protocol_envelopes.rs               │ Removed 3 packed ProtocolEnvelope impls, added 3 unpacked impls (infallible get_nested); updated get_newest_envelope_response::Response to convert packed→unpacked at boundary; updated tests │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/protocol/extractors/bytes.rs                       │ visit_originator takes &UnpackedOriginatorEnvelope                                                                                                                                            │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/protocol/extractors/cursor.rs                      │ visit_unsigned_originator takes &UnpackedUnsignedOriginatorEnvelope                                                                                                                           │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/protocol/extractors/group_messages.rs              │ Same                                                                                                                                                                                          │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/protocol/extractors/timestamp.rs                   │ Same                                                                                                                                                                                          │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/protocol/extractors/welcomes.rs                    │ Same                                                                                                                                                                                          │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/protocol/extractors/identity_updates.rs            │ Same                                                                                                                                                                                          │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/endpoints/d14n/query_envelopes.rs                  │ type Output = UnpackedQueryEnvelopesResponse                                                                                                                                                  │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_envelopes.rs              │ type Output = UnpackedSubscribeEnvelopesResponse                                                                                                                                              │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/queries/d14n/identity.rs                           │ let result: UnpackedQueryEnvelopesResponse = ...                                                                                                                                              │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/queries/d14n/streams.rs                            │ Updated PagedItem, OrderedStreamT, WelcomeMessageStream type aliases                                                                                                                          │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/protocol/extractors/test_utils/envelope_builder.rs │ build() returns UnpackedOriginatorEnvelope directly                                                                                                                                           │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_api_d14n/src/protocol/resolve/network_backoff.rs                │ Test fixtures use UnpackedQueryEnvelopesResponse                                                                                                                                              │
  ├─────────────────────────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ crates/xmtp_mls/src/subscriptions/d14n_compat.rs                            │ V3OrD14n::D14n holds UnpackedSubscribeEnvelopesResponse; decode() decodes as that type                                                                                                        │
  └─────────────────────────────────────────────────────────────────────────────┴───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

  ---
  Environment Notes

  The nix develop shell to use:
  nix develop /workspace#rust --accept-flake-config --command <cargo command>

  ---
  Next Steps (what to do now)

  # 1. Verify compilation
  just check
   # 2. Run tests
  just test crate xmtp_proto
  just test crate xmtp_api_d14n

  # 3. Lint
just lint

  # 4. Commit
commit changes

  # 5. Create PR
  gh pr create --title "..." --body "..."

  The commit message should describe: "Eliminate redundant decode calls by defining Unpacked protobuf variants that inline nested byte fields at the same tag numbers, enabling single-pass deserialization and infallible visitor traversal."
