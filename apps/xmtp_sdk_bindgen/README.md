# XMTP SDK generator

`xmtp-sdk-bindgen` writes the Swift, Kotlin, Node, and browser bindings of
`crates/xmtp_sdk`. Run it through `dev/nix-shell 'just sdk generate'`; see
`crates/xmtp_sdk/AGENTS.md` for the recipes.

A routine façade change (a method, a getter, an event variant or payload
record, a Client static, a record field, including one of `MessageData`)
needs no edit here.
The façade describes its items with `#[xmtp_macro::sdk_export]`, and the
generator reads the result from the UniFFI library metadata.

## Metadata markers

The macro turns its `pure` and `client_static` arguments and `#[sdk(...)]`
member options into `#[doc = "@xmtp-..."]` lines. UniFFI carries docstrings
into the library metadata, `src/markers.rs` reads them, and the generator
strips them from generated documentation comments; code and string literals
keep their text.
`dev/nix-shell 'just sdk lint'` fails if a marker reaches a binding. UniFFI
includes docstrings in its API checksums, so a new marker changes the
checksum of its function; the library and its bindings are always generated
together.

| Marker | Façade source | What the generator does |
| --- | --- | --- |
| `@xmtp-pure` | `#[sdk_export(pure)]` on a sync free function | Puts it in the browser's main-thread pure module, outside the worker bridge |
| `@xmtp-client-static` | `#[sdk_export(client_static)]` on an async free function | Adds a static that calls the function to the Client of every SDK; see [Client statics](#client-statics) |
| `@xmtp-worker` | Written in a doc comment | Keeps a call the browser worker makes itself off the bridge |
| `@xmtp-internal` | Written in a doc comment | Leaves the item out of the public projection |
| `@xmtp-host-internal` | `#[sdk(host_internal)]` on an object method | Keeps the binding method private to the host runtime and removes its native interface entry; also writes `@xmtp-internal` |
| `@xmtp-immutable` | `#[sdk(immutable)]` on a sync `&self` getter | Lets the browser bridge read the getter once, from a snapshot |
| `@xmtp-kind=name` | `#[sdk(kind = "name")]` on each `EventKind` variant | Uses `name` as the public TypeScript string of the event kind |
| `@xmtp-redact`, `@xmtp-redact=key` | `#[sdk(redact)]` or `#[sdk(redact = "key")]` on a record or variant field | Hides the value, or that key of a string map, in the generated Kotlin `toString` and the Swift `description` that `runtime/RecordDescriptions.swift` holds |
| `@xmtp-redacted` | The record or enum of a `#[sdk(redact)]` field | Admits that type's `@xmtp-redact` markers; without it, generation stops |

A marker is a whole word of a docstring: `@xmtp-`, a lowercase name, and an
optional `=value`. The generator stops on such a word outside the table, so a
misspelling such as `@xmtp-interal` cannot leave a private item public. A
value reaches generated string literals, so the generator also stops on a
kind or a redacted map key outside the grammar of its `#[sdk(...)]` option
and on a value given to any other marker. Only `@xmtp-worker` and
`@xmtp-internal` are written by hand. The others written in a doc comment
would skip the macro's checks: `sdk_export` rejects them in the items it
exports, and an `xmtp_macro` test rejects them anywhere in the façade source,
along with a `doc` value that is not a string literal.

`#[sdk_export(native_only)]` and `#[sdk_export(wasm_only)]` write no marker.
They are the target's `#[cfg]` above the item. `#[sdk(shown)]` writes none
either: it marks a field that diagnostic text prints beside a redacted one.

Rules the macro enforces at compile time:

- A synchronous `&self` method without arguments that returns a value, not a
  `Result`, needs `#[sdk(immutable)]` unless the browser bridge never
  forwards it: its doc comment says `@xmtp-worker`, or its item is
  `native_only`. The conformance constructor probe reads live state through
  native-only getters. Mark a getter only when its value never changes for
  the object's lifetime; otherwise make the read async.
- `#[sdk(kind)]` marks every variant of an enum, each with its own kind, or
  none.
- `client_static` needs an asynchronous free function and excludes `pure`,
  `native_only`, and `wasm_only`: every SDK has the static.
- Redaction fails closed. Once a record, or any variant of an enum, has a
  `#[sdk(redact)]` field, every other field of the type takes
  `#[sdk(redact)]` or `#[sdk(shown)]`, so a new field or variant never prints
  a secret by default. The macro implements
  `Debug` for the record or enum by calling its `redacted_debug` method,
  which the façade writes to hide the same fields. Any other `Debug`
  conflicts with it, so a derived one fails to compile wherever the derive
  sits. A `redact = "key"` key holds only ASCII letters, digits, `_`, `.`,
  and `-`, both options need a named field, and a `uniffi::Error` type
  cannot redact, because the Kotlin binding renames it to an exception class.
- On a record or enum, `sdk_export` is the first attribute, above every
  derive: a derive written above it expands first, out of the macro's sight.
  When rustc reports "cannot find attribute `sdk` in this scope", the item
  lacks `#[xmtp_macro::sdk_export]` as its first attribute.

The generator checks the key again: it stops when `redact = "key"` marks
anything but a map with string keys and values, or when the key holds a
character outside that set.

Derived without a marker:

- Kotlin prints a byte field of a redacted record as its length
  (`contentBytes=16`), so a payload never reaches diagnostic text. Swift
  prints a byte count by default.
- Each `ClientEvent` variant takes the kind of the `EventKind` variant with
  its name. Generation stops when the two enums' variants differ, and the
  error names the missing ones.
- Event payload records are the records that `ClientEvent` variants and
  `EventFilter` reach. Their TypeScript fields keep the Rust spelling, as the
  events spec writes them. A record that another call also reaches stops
  generation: give the event its own record.
- Enums that the same payloads reach use snake_case values, as the events
  spec writes them (`deleted_locally`). Other enums use camelCase values. An
  enum that another call also reaches stops generation when a value without
  a kind marker reads differently in the two spellings: give the event its
  own enum. One-word values, as in `ConnectionState`, read the same.
- Swift cancellation cleanup: when the caller of an async `Client`
  constructor is cancelled, the future discards the ready `Client` it never
  returned. `src/swift_async.rs` names the two conformance probe calls that
  also build a `Client`; any other future that lifts a `Client` stops
  generation, because it may return a `Client` that the caller does not own.

## Client statics

`#[sdk_export(client_static)]` marks an asynchronous free function that every
SDK also exposes as a static member of its Client. The static's name is the
function's without a trailing `_with_backend`, in the SDK's casing. It takes
the function's parameters in Rust order with the `BackendSource` parameter
moved last, and the function stays exported. So
`can_message_with_backend(backend, identities)` becomes
`Client.canMessage(identities, backend)` in TypeScript,
`SDKClient.canMessage(identities, backend)` in Kotlin, and
`SDKClient.canMessage(identities:backend:)` in Swift.

- TypeScript: the generated `ClientMembers` base, which the public `Client`
  extends, gets a static that calls the public function.
- Kotlin: `runtime/ClientForwarding.kt` gets an extension of
  `SDKClient.Companion`, with the binding's parameter and result types. A
  `BackendSource` or a foreign trait passes through its `SDKForeign` wrapper.
- Swift: `runtime/ClientForwarding.swift` gets a `static func` with every
  parameter labelled, with the binding's types.

Generation stops on a function that the rule cannot express:

- a synchronous or `@xmtp-internal` one;
- one with two `BackendSource` parameters or a defaulted `BackendSource`;
- one with a defaulted parameter that the moved `BackendSource` would
  follow, because a TypeScript caller could not leave that parameter out;
- a name that another static, the host constructors `create` and `build`,
  the class `constructor`, a property of every JavaScript function
  (`name`, `length`, `prototype`, `caller`, `arguments`), or a Swift
  declaration keyword (`init`, `deinit`, `subscript`) takes;
- a binding declaration whose parameters differ from the metadata;
- a Kotlin parameter that holds a foreign trait or a `BackendSource` that no
  `SDKForeign` wrapper reaches: a foreign trait without a wrapper, or one in
  a container, a record, or an enum.

## Message fields

Each SDK's `Message` wraps a `MessageData` record. The generator writes an
accessor for every field but two that the host reads itself: `client_key`
names the client that returned the message, and the host decodes `content`
with that client's codecs. Generation stops when either one is missing.

- TypeScript: `message-fields.gen.ts` holds the getters of the binding
  message that the Node runtime's and the browser host's `Message` extend.
  The public `Message` extends the `MessageFields` class of the public
  projection, which lifts each field to its public value once. A delivery
  cursor reads as `null` when absent.
- Kotlin: `runtime/MessageFields.kt` is the base class of `Message`, with
  value equality and a hash over every `MessageData` field. Byte arrays,
  including those in enum variants and their records, compare by content;
  every other value compares as its class does.
- Swift: `runtime/MessageFields.swift` extends `Message` with an accessor per
  field, of the type that the binding's `MessageData` struct declares.

The hand-written `Message` classes keep only content and reply decoding and
the message actions.

## Hand-maintained areas

These stay outside the markers on purpose:

- Codecs (`runtime/*/SDKCodecs*`, `runtime/ts/codecs.ts`): each codec needs
  a hand-written runtime class per language, and the name lists sit beside
  those classes.
- Public projection policy (`src/public_projection/policy.rs`): the
  `CREDENTIAL_GUARD` text and the `DELIVERY_CURSOR_*` lists.
- The `MessageData` fields that the hand-written `Message` reads itself
  (`HOST_FIELDS` in `src/message_fields.rs`): `client_key` and `content`.
- Codec sends (`is_codec_send` in `src/public_projection/objects.rs`) and
  host client methods (`HOST_CLIENT_METHODS` in `src/forwarding.rs`).
- Rust `Debug` of a redacted record (`redacted_debug` in the façade): the
  façade writes it, and its own tests cover it.
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
