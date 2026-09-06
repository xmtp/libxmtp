# Protobuf sources

This directory is the source of truth for XMTP protobuf schemas.

## Source provenance

- XMTP schemas were copied from `xmtp/proto` commit `dedb87251f23bee8133154706afbc0aa1348210d`. The unused `keystore_api/v1/keystore.proto` schema was not copied.
- `google/api/annotations.proto` and `google/api/http.proto` were copied from Buf module `buf.build/googleapis/googleapis` commit `cc916c31859748a68fd229a3c8d7a2e8`. See `LICENSE.googleapis`.
- `protoc-gen-openapiv2/options/annotations.proto` and `protoc-gen-openapiv2/options/openapiv2.proto` were copied from Buf module `buf.build/grpc-ecosystem/grpc-gateway` commit `a1ecdc58eccd49aa8bea2a7a9022dc27`. See `LICENSE.grpc-gateway`.

The approved self-hosted API schema is `backend/v1/backend.proto`.
