# XMTP SDK generator

`xmtp-sdk-bindgen` writes the Swift, Kotlin, Node, and browser bindings of
`crates/xmtp_sdk`. Run it through `dev/nix-shell 'just sdk generate'`; see
`crates/xmtp_sdk/AGENTS.md` for the recipes.

A routine façade change (a method, a getter, an event variant or payload
record, a record field other than one of `MessageData`) needs no edit here.
The façade describes its items with `#[xmtp_macro::sdk_export]`, and the
generator reads the result from the UniFFI library metadata.

## Metadata markers

The macro turns its `pure` argument and `#[sdk(...)]` member options into
`#[doc = "@xmtp-..."]` lines. UniFFI carries docstrings into the library
metadata, `src/markers.rs` reads them, and the generator strips them from
generated documentation comments; code and string literals keep their text.
`dev/nix-shell 'just sdk lint'` fails if a marker reaches a binding. UniFFI
includes docstrings in its API checksums, so a new marker changes the
checksum of its function; the library and its bindings are always generated
together.

| Marker | Façade source | What the generator does |
| --- | --- | --- |
| `@xmtp-pure` | `#[sdk_export(pure)]` on a sync free function | Puts it in the browser's main-thread pure module, outside the worker bridge |
| `@xmtp-worker` | Written in a doc comment | Keeps a call the browser worker makes itself off the bridge |
| `@xmtp-internal` | Written in a doc comment | Leaves the item out of the public projection |
| `@xmtp-immutable` | `#[sdk(immutable)]` on a sync `&self` getter | Lets the browser bridge read the getter once, from a snapshot |
| `@xmtp-kind=name` | `#[sdk(kind = "name")]` on each `EventKind` variant | Uses `name` as the public TypeScript string of the event kind |

A marker is a whole word of a docstring: `@xmtp-`, a lowercase name, and an
optional `=value`. The generator stops on such a word outside the table, so a
misspelling such as `@xmtp-interal` cannot leave a private item public.

`#[sdk_export(native_only)]` and `#[sdk_export(wasm_only)]` write no marker.
They are the target's `#[cfg]` above the item.

Rules the macro enforces at compile time:

- A synchronous `&self` method without arguments that returns a value, not a
  `Result`, needs `#[sdk(immutable)]` unless the browser bridge never
  forwards it: its doc comment says `@xmtp-worker`, or its item is
  `native_only`. The conformance constructor probe reads live state through
  native-only getters. Mark a getter only when its value never changes for
  the object's lifetime; otherwise make the read async.
- `#[sdk(kind)]` marks every variant of an enum, each with its own kind, or
  none.
- On a record or enum, `sdk_export` is the first attribute, above every
  derive: a derive written above it expands first, out of the macro's sight.
  When rustc reports "cannot find attribute `sdk` in this scope", the item
  lacks `#[xmtp_macro::sdk_export]` as its first attribute.

Derived without a marker:

- Each `ClientEvent` variant takes the kind of the `EventKind` variant with
  its name. Generation stops when the two enums' variants differ, and the
  error names the missing ones.
- Event payload records are the records that `ClientEvent` variants and
  `EventFilter` reach. Their TypeScript fields keep the Rust spelling, as the
  events spec writes them. A record that another call also reaches stops
  generation: give the event its own record.
- Enums that the same payloads reach use snake_case values, as the events
  spec writes them (`deleted_locally`). Other enums use camelCase values.
- Swift cancellation cleanup: when the caller of an async `Client`
  constructor is cancelled, the future discards the ready `Client` it never
  returned. `src/swift_async.rs` names the two conformance probe calls that
  also build a `Client`; any other future that lifts a `Client` stops
  generation, because it may return a `Client` that the caller does not own.

## Hand-maintained areas

These stay outside the markers on purpose:

- Codecs (`runtime/*/SDKCodecs*`, `runtime/ts/codecs.ts`): each codec needs
  a hand-written runtime class per language, and the name lists sit beside
  those classes.
- `Message` accessors over `MessageData` (five runtime files, such as
  `runtime/ts/message.ts` and `runtime/kotlin/SDKTypes.kt`): each language
  reads the record's fields by hand. A later change generates them.
- The `*_with_backend` `Client` statics (five hand copies in the Swift,
  Kotlin, and TypeScript runtimes and the browser bridge templates). A later
  change generates them.
- Public projection policy (`src/public_projection/policy.rs`): the
  `CREDENTIAL_GUARD` text and the `DELIVERY_CURSOR_*` lists.
- Codec sends (`is_codec_send` in `src/public_projection/objects.rs`) and
  host client methods (`HOST_CLIENT_METHODS` in `src/forwarding.rs`).
- Kotlin record diagnostics (`src/kotlin_records.rs`): it pins the fields of
  each record whose `toString` hides a secret.
- Foreign callback results (`src/callback_results.rs`): the TypeScript
  callback rewrite pins the error and result types of the foreign-trait
  methods, so a new callback updates both lists.
- Streams and readers (`runtime/*/streams`): host iterator, cancellation, and
  acknowledgement behaviour is per language, not metadata.
- Events host behaviour (`runtime/*/events`): listener and iterator lifetimes
  belong to the host runtime.
- Logging (`runtime/ts/logging.ts`, `templates/bridge/logging.ts`,
  `src/logging_admission.rs`): the log sink handoff differs per host.
- Apple and Android lifecycle (`runtime/swift/AppleLifecycle.swift`,
  `runtime/android`): platform callbacks with no façade item.
- Identity routes (`IdentityRoutes.*`, `src/public_projection/identity.rs`,
  `src/forwarding.rs`): four routes fixed by the SDK design; a new route is a
  spec change.
- Template-text rewrites (`src/callback_cursor.rs`, `src/kotlin_callbacks.rs`,
  `src/swift_async.rs`, `src/swift_events.rs`, `src/native_visibility.rs`,
  `src/swift_records.rs`): they pin stock UniFFI template text and change
  with the UniFFI version, not with façade edits.
