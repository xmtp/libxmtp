# Changelog

All notable changes to this project are documented here.

## [unreleased]

### Features

- *(telemetry)* Add bidi.* transport spans (#3932)
- *(telemetry)* Add context and caller spans to the reference-ID parse warn (#3938)
- *(telemetry)* Export spans across send/sync/welcome/stream paths; log swallowed errors (#3939)
- *(agent-sdk)* Migrate @xmtp/agent-sdk from xmtp-js (#3979)
- *(xmtp_db)* Add IntentState::Superseded for abandoned guarded intents (#4019)
- *(xmtp_mls)* Report app data changes and guard updates against clobbering (#4011)
- *(bindings_mobile)* Expose the app data change callback and update guard (#4012)
- *(android-sdk)* Surface app data change callbacks and the update guard (#4016)
- *(bindings_wasm)* Expose the app data change callback and update guard (#4014)
- *(bindings_node)* Expose the app data change callback and update guard (#4013)
- *(node-sdk)* Surface app data change callbacks and the update guard (#4017)
- *(ios-sdk)* Surface app data change callbacks and the update guard (#4015)
- *(release)* Automated consumer bump PRs for dev releases (#4027)
- *(release)* Unified semver ordering for main-cut prereleases (#4048)
- *(logging)* Sentry telemetry backend (core plumbing) (#4049)
- *(bindings/mobile)* Uniffi surface for Sentry telemetry (#4050)
- *(xmtp_mls)* Bound app-data change callback with a timeout (#4052)

### Bug Fixes

- *(ci)* Accept release/vX.Y maintenance branches in release-notes validation (#3924)
- *(scw)* Replace panic with error on empty API response in signature verification (#3927)
- *(xmtp_api_grpc)* Relax h2 keepalive defaults to stop bidi stream churn (#3933)
- *(xmtp_api_d14n)* Track in-wave bidi replay progress per topic (#3934)
- Preserve ordered welcome cursor retries (#3931)
- *(xmtp_common)* Stop Android device-sync abort by using webpki roots (the gRPC path already does this) (#3940)
- *(ios)* Remove duplicate onTermination in Conversations.stream() (#3507)
- *(node)* Surface failed_installations from addMembers (#3763) (#3944)
- *(xmtp_mls)* Skip phantom members on partial key package failure (#3945) (#3946)
- *(xmtp_mls)* Treat membership sequence_id 0 as unset when resolving association state (#3948)
- *(xmtp_api_d14n)* Surface extraction errors instead of silently dropping them (#3961)
- *(xmtp_mls)* Silence unused_mut warning without breaking test-utils build (#3962)
- *(xmtp_mls)* Detect unprocessed MLS commits in filter_groups_with_new_messages (#3959)
- *(xmtp_mls)* Record publish-time key package failures in group membership (#3949)
- *(deps)* Bump lru to 0.18.2 for RUSTSEC-2026-0253 (#3967)
- Fix consent prefs sync test (#3965)
- *(node-sdk,browser-sdk)* Make stream end() terminal and race-safe in createStream (#3978)
- *(node-sdk,browser-sdk)* Reschedule when the native stream closes during restart (#3980)
- *(xmtp_mls)* Defer unknown task kinds instead of deleting them (#4020)
- *(logging)* Caller-supplied component tag wins over the libxmtp default (#4053)

### Performance

- *(xmtp_mls)* Read group metadata from GroupContext, not a full MlsGroup load (#4023)

### Miscellaneous

- Bump manifests to next dev versions after 1.11.0 (#3923)
- *(ci)* Consolidate nix sticky disks into closure families (#3935)
- *(deps)* Cargo update ruint security advisory (#3942)
- *(deps-dev)* Bump the node-dev group across 1 directory with 2 updates (#3957)
- *(xmtp_proto)* Regenerate protos for the metadata update guard (#4018)
- *(deny)* Patch h2 to 0.4.16, ignore residual test-only advisory (RUSTSEC-2026-0258) (#4024)
- *(logging)* Normalize tracing attribute keys across the workspace (#4046)
- *(android)* Raise integration test timeouts (#4044) (#4054)

### Other

- Deflake should stream prefs (#3964)
- H2 0.4.16 -> 0.4.19 (#4043)

