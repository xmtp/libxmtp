# XMTP backend API

- [src/endpoints/backend/](src/endpoints/backend/) implements backend endpoints.
- [src/queries/backend/](src/queries/backend/) owns backend binding and transport.
- [src/queries/bidi.rs](src/queries/bidi.rs) and
  [src/queries/bidi_transport.rs](src/queries/bidi_transport.rs) implement shared bidi transport.
- [src/queries/bidi_transport/tests/](src/queries/bidi_transport/tests/) and
  [src/queries/bidi_transport_props.rs](src/queries/bidi_transport_props.rs) hold transport tests.
- [src/test/bidi.rs](src/test/bidi.rs) provides the scripted bidi peer for transport tests.
