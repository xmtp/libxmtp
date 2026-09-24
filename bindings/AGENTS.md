# Bindings

Bindings translate the core API. Keep business logic in `xmtp_mls` or a shared
crate. Use the existing `ErrorCode` conversion and shared stream failure encoder;
do not parse ordinary error messages for structured state.

Auth callback failures return only `auth callback failed`. Do not retain or log
callback error text or credentials. The middleware owns retryability.

Use the platform's `xmtp_macro` builder. Supported field attributes are
`#[builder(required)]`, `#[builder(optional)]`, `#[builder(default = "expr")]`,
and `#[builder(skip)]`. Write `build()` by hand.

Message readers carry an opaque acknowledgement token. Check ownership just
before app handoff; reject a stale selection and read again. Queue insertion
does not acknowledge delivery. A callback acknowledges after success; an
iterator acknowledges when the app requests the next item. Closing or dropping
a reader does not acknowledge a pending item. Replay from a delivery cursor
does not change default progress. History and its cursor come from one database
snapshot. Preserve storage errors and the saved acknowledgement state.
Fence old tokens on terminal reader failure; callers can reopen after storage
repair.

Node and WASM exports use matching bare names for the same API surface.
