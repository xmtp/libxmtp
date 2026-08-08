# Changelog

All notable changes to this project are documented here.

## [unreleased]

### Features

- *(telemetry)* Add bidi.* transport spans (#3932)
- *(telemetry)* Add context and caller spans to the reference-ID parse warn (#3938)
- *(telemetry)* Export spans across send/sync/welcome/stream paths; log swallowed errors (#3939)

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

### Miscellaneous

- Bump manifests to next dev versions after 1.11.0 (#3923)
- *(ci)* Consolidate nix sticky disks into closure families (#3935)
- *(deps)* Cargo update ruint security advisory (#3942)

