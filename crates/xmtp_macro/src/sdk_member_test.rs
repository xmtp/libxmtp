//! Token-stream tests of the `#[sdk(...)]` member options.

use quote::quote;

use crate::sdk_export_test::{compact, error, export};

// Getters: the SDKs expose a synchronous, argument-free, infallible method as
// a readonly property, and the browser bridge reads it from a snapshot. The
// façade author must say that the value never changes.

#[test]
fn immutable_getter_emits_marker_and_removes_sdk_attribute() {
    let output = compact(&export(
        quote!(),
        quote! {
            impl Group {
                /// The conversation ID.
                #[sdk(immutable)]
                pub fn id(&self) -> ConversationId { self.id.clone() }
                pub async fn sync(&self) -> Result<(), Error> { Ok(()) }
            }
        },
    ));
    assert_eq!(output.matches("#[doc=\"@xmtp-immutable\"]").count(), 1);
    assert!(!output.contains("sdk(immutable)"));
    // The doc comment stays; the marker is one more doc line.
    assert!(output.contains("#[doc=r\"TheconversationID.\"]"));

    // UniFFI also hands a method `self: Arc<Self>`.
    let output = export(
        quote!(),
        quote!(impl Group { #[sdk(immutable)] pub fn id(self: Arc<Self>) -> ConversationId { self.id.clone() } }),
    );
    assert!(output.contains("@xmtp-immutable"));
}

#[test]
fn unmarked_sync_getter_is_rejected_with_the_fix() {
    for item in [
        quote!(impl Client { pub fn inbox_id(&self) -> InboxId { self.inbox_id.clone() } }),
        quote!(
            trait Api {
                fn inbox_id(&self) -> InboxId;
            }
        ),
        quote!(impl Client { pub fn inbox_id(self: Arc<Self>) -> InboxId { self.inbox_id.clone() } }),
    ] {
        for target in [quote!(), quote!(wasm_only)] {
            let message = error(target, item.clone());
            assert!(
                message.contains("`inbox_id` is a synchronous getter"),
                "{message}"
            );
            assert!(message.contains("#[sdk(immutable)]"));
            assert!(message.contains("make it async"));
        }
    }
}

// The browser bridge forwards neither a native-only item nor a worker-only
// call, so their getters need no snapshot. The conformance constructor probe
// reads live state through them.
#[test]
fn getters_the_bridge_never_forwards_may_read_live_values() {
    let output = export(
        quote!(native_only),
        quote!(impl Probe { pub fn ready_client_alive(&self) -> bool { false } }),
    );
    assert!(!output.contains("@xmtp-"));
    let output = export(
        quote!(),
        quote!(impl Probe {
            /// Read the private native shutdown boundary. @xmtp-worker @xmtp-internal
            pub fn shutdown_entered(&self) -> bool { false }
        }),
    );
    assert!(!output.contains("@xmtp-immutable"));
    // The marker is a whole word of the doc comment.
    let message = error(
        quote!(),
        quote!(impl Probe {
            /// See @xmtp-workers.
            pub fn shutdown_entered(&self) -> bool { false }
        }),
    );
    assert!(message.contains("`shutdown_entered` is a synchronous getter"));
}

#[test]
fn sync_methods_that_are_not_getters_need_no_marker() {
    // Arguments, a Result, no return value, a receiver that mutates or
    // consumes, or no receiver: none of these is a snapshot property.
    let output = export(
        quote!(),
        quote! {
            impl Client {
                #[uniffi::constructor]
                pub fn new() -> Self { Self }
                pub fn lookup(&self, key: u64) -> Option<String> { None }
                pub fn state(&self) -> Result<State, Error> { Ok(State) }
                pub fn reset(&self) {}
                pub fn touch(&self) -> () {}
                pub fn bump(&mut self) -> u64 { 0 }
                pub fn into_id(self) -> u64 { 0 }
            }
        },
    );
    assert!(!output.contains("@xmtp-"));
}

#[test]
fn immutable_rejects_methods_that_are_not_getters() {
    for item in [
        quote!(impl Client { #[sdk(immutable)] pub async fn id(&self) -> u64 { 0 } }),
        quote!(impl Client { #[sdk(immutable)] pub fn id(&self) -> Result<u64, Error> { Ok(0) } }),
        quote!(impl Client { #[sdk(immutable)] pub fn id(&self, key: u64) -> u64 { key } }),
        quote!(impl Client { #[sdk(immutable)] pub fn id(&self) {} }),
        quote!(impl Client { #[sdk(immutable)] pub fn id(&mut self) -> u64 { 0 } }),
        quote!(impl Client { #[sdk(immutable)] pub fn id(self: &mut Self) -> u64 { 0 } }),
        quote!(impl Client { #[sdk(immutable)] pub fn id(self) -> u64 { 0 } }),
        quote!(impl Client { #[sdk(immutable)] pub fn id(mut self) -> u64 { 0 } }),
        quote!(impl Client { #[sdk(immutable)] #[uniffi::constructor] pub fn new() -> Self { Self } }),
    ] {
        let message = error(quote!(), item);
        assert!(
            message.contains("#[sdk(immutable)] needs a synchronous `&self` method"),
            "{message}"
        );
    }
}

// Kinds: the public string of a variant, such as an event kind.

#[test]
fn kinds_mark_every_variant_once_or_none() {
    let output = compact(&export(
        quote!(),
        quote! {
            #[derive(Clone, Copy, uniffi::Enum)]
            pub enum EventKind {
                #[sdk(kind = "conversation.joined")]
                ConversationJoined,
                #[sdk(kind = "hmac_keys.updated")]
                HmacKeysUpdated,
            }
        },
    ));
    assert!(output.contains("#[doc=\"@xmtp-kind=conversation.joined\"]ConversationJoined"));
    assert!(output.contains("#[doc=\"@xmtp-kind=hmac_keys.updated\"]HmacKeysUpdated"));
    assert!(!output.contains("sdk("));
    // The derive exports the enum; the macro adds no export or span.
    assert!(output.starts_with("#[derive(Clone,Copy,uniffi::Enum)]"));
    assert!(!output.contains("uniffi::export"));
    assert!(!output.contains("tracing"));

    let message = error(
        quote!(),
        quote! {
            #[derive(uniffi::Enum)]
            pub enum EventKind {
                #[sdk(kind = "conversation.joined")]
                ConversationJoined,
                Lagged,
            }
        },
    );
    assert!(message.contains("`EventKind`: every variant needs #[sdk(kind"));
    assert!(message.contains("`Lagged` has none"));

    let message = error(
        quote!(),
        quote! {
            #[derive(uniffi::Enum)]
            pub enum EventKind {
                #[sdk(kind = "lagged")]
                Lagged,
                #[sdk(kind = "lagged")]
                Dropped,
            }
        },
    );
    assert!(message.contains("kind is repeated in this enum"));

    // An enum without kinds keeps its default public names.
    let output = export(
        quote!(),
        quote!(
            #[derive(uniffi::Enum)]
            pub enum JoinOrigin {
                Created,
                Welcomed,
            }
        ),
    );
    assert!(!output.contains("@xmtp-"));

    for bad in [
        "",
        "Conversation.Joined",
        "conversation joined",
        "conversation-joined",
    ] {
        let message = error(
            quote!(),
            quote!(
                #[derive(uniffi::Enum)]
                pub enum EventKind {
                    #[sdk(kind = #bad)]
                    A,
                }
            ),
        );
        assert!(
            message.contains("kind takes lowercase words"),
            "{bad:?}: {message}"
        );
    }
}

#[test]
fn member_options_reject_the_wrong_member() {
    let cases: [(proc_macro2::TokenStream, &str); 6] = [
        (
            quote!(impl Client { #[sdk(kind = "a.b")] pub async fn sync(&self) {} }),
            "#[sdk(kind = ...)] applies to enum variants",
        ),
        (
            quote!(impl Client { #[sdk(native_only)] pub async fn sync(&self) {} }),
            "unknown sdk option; expected immutable, host_internal, stream(...), kind",
        ),
        (
            quote!(
                #[derive(uniffi::Record)]
                pub struct Key {
                    #[sdk(immutable)]
                    pub bytes: Vec<u8>,
                }
            ),
            "#[sdk(immutable)] applies to methods",
        ),
        (
            quote!(
                #[derive(uniffi::Record)]
                pub struct Key {
                    #[sdk(kind = "a")]
                    pub bytes: Vec<u8>,
                }
            ),
            "#[sdk(kind = ...)] applies to enum variants",
        ),
        (
            quote!(
                #[derive(uniffi::Enum)]
                pub enum Channel {
                    Apns {
                        #[sdk(immutable)]
                        token: String,
                    },
                }
            ),
            "#[sdk(immutable)] applies to methods",
        ),
        (
            quote!(
                #[sdk(immutable)]
                pub async fn run() {}
            ),
            "`run` is a free function; it takes its options in #[sdk_export(...)]",
        ),
    ];
    for (item, expected) in cases {
        let message = error(quote!(), item);
        assert!(message.contains(expected), "{expected}: {message}");
    }
    let message = error(
        quote!(),
        quote!(impl Client { #[sdk(frozen)] pub fn id(&self) -> u64 { 0 } }),
    );
    assert!(message.contains("unknown sdk option; expected immutable"));
    let message = error(
        quote!(),
        quote!(
            #[derive(uniffi::Record)]
            pub struct Key {
                #[sdk(native_only)]
                pub bytes: Vec<u8>,
            }
        ),
    );
    assert!(
        message
            .contains("unknown sdk option; expected immutable, host_internal, stream(...), kind")
    );
    let message = error(
        quote!(),
        quote!(impl Client {
            #[sdk(immutable)]
            #[sdk(immutable)]
            pub fn id(&self) -> u64 { 0 }
        }),
    );
    assert!(message.contains("sdk option is repeated"));
}

// Redaction: a redacted field stays out of generated Kotlin and Swift
// diagnostic text. A map field can hide one key.

#[test]
fn record_and_variant_fields_emit_redact_markers() {
    let output = compact(&export(
        quote!(),
        quote! {
            #[derive(Clone, uniffi::Record)]
            pub struct Credential {
                #[sdk(shown)]
                pub name: Option<String>,
                #[sdk(redact)]
                pub value: String,
                #[sdk(redact = "x-secret.v1_2")]
                pub parameters: HashMap<String, String>,
                #[cfg(not(target_arch = "wasm32"))]
                #[sdk(redact)]
                pub key: Option<Vec<u8>>,
            }
        },
    ));
    assert!(output.contains("#[doc=\"@xmtp-redact\"]pubvalue:String"));
    assert!(output.contains("#[doc=\"@xmtp-redact=x-secret.v1_2\"]pubparameters"));
    assert!(output.contains("#[cfg(not(target_arch=\"wasm32\"))]#[doc=\"@xmtp-redact\"]pubkey"));
    assert!(output.contains("#[doc=\"@xmtp-redacted\"]pubstructCredential"));
    assert_eq!(output.matches("@xmtp-").count(), 4);
    assert!(!output.contains("sdk("));

    let output = compact(&export(
        quote!(native_only),
        quote! {
            #[derive(Clone, uniffi::Enum)]
            pub enum NotificationChannel {
                Apns { #[sdk(redact)] token: String },
                Http { #[sdk(redact)] url: String, #[sdk(shown)] signing_key: Vec<u8> },
                Disabled,
            }
        },
    ));
    assert_eq!(output.matches("#[doc=\"@xmtp-redact\"]").count(), 2);
    assert!(output.contains("#[doc=\"@xmtp-redact\"]url:String,signing_key"));
}

#[test]
fn redact_applies_to_named_fields_with_a_plain_key() {
    let record = |field: proc_macro2::TokenStream| {
        quote!(
            #[derive(uniffi::Record)]
            pub struct Envelope {
                #field
                pub parameters: HashMap<String, String>,
            }
        )
    };
    let mut cases = vec![
        (
            quote!(impl Client { #[sdk(redact)] pub async fn sync(&self) {} }),
            "#[sdk(redact)] and #[sdk(shown)] apply to record and variant fields",
        ),
        (
            quote!(impl Client { #[sdk(shown)] pub async fn sync(&self) {} }),
            "#[sdk(redact)] and #[sdk(shown)] apply to record and variant fields",
        ),
        (
            quote!(
                #[derive(uniffi::Enum)]
                pub enum Channel {
                    #[sdk(redact)]
                    Apns { token: String },
                }
            ),
            "#[sdk(redact)] and #[sdk(shown)] apply to record and variant fields",
        ),
        (
            quote!(
                #[sdk(redact)]
                pub async fn run() {}
            ),
            "`run` is a free function",
        ),
        (
            quote!(
                #[derive(uniffi::Record)]
                pub struct Key(#[sdk(redact)] pub Vec<u8>);
            ),
            "#[sdk(redact)] and #[sdk(shown)] need a named field",
        ),
        (
            quote!(
                #[derive(uniffi::Enum)]
                pub enum Channel {
                    Apns(#[sdk(shown)] String),
                }
            ),
            "#[sdk(redact)] and #[sdk(shown)] need a named field",
        ),
        (
            record(quote!(#[sdk(redact, shown)])),
            "a field is either #[sdk(redact)] or #[sdk(shown)]",
        ),
        (
            record(quote!(#[sdk(redact, redact = "secret")])),
            "sdk option is repeated",
        ),
        (
            record(quote!(#[sdk(shown)])),
            "#[sdk(shown)] applies in a type with a #[sdk(redact)] field",
        ),
    ];
    // The key reaches a Kotlin and a Swift string literal as it is.
    for key in ["", "a key", "a\"b", "a\\b", "$secret", "${x}", "ключ"] {
        cases.push((
            record(quote!(#[sdk(redact = #key)])),
            "redact takes one map key of ASCII letters, digits, `_`, `.`, and `-`",
        ));
    }
    for (item, expected) in cases {
        let message = error(quote!(), item);
        assert!(message.contains(expected), "{expected}: {message}");
    }
}

// Redaction fails closed: in a type with a redacted field, every field says
// whether it is redacted or shown, so a new field cannot print a secret by
// default.
#[test]
fn every_field_beside_a_redacted_one_is_redacted_or_shown() {
    let message = error(
        quote!(),
        quote! {
            #[derive(Clone, uniffi::Record)]
            pub struct Credential {
                #[sdk(shown)]
                pub name: Option<String>,
                #[sdk(redact)]
                pub value: String,
                pub expires_at_seconds: i64,
            }
        },
    );
    assert_eq!(
        message,
        "`Credential.expires_at_seconds` is in a type with a redacted field; mark it #[sdk(redact)] or #[sdk(shown)]"
    );
    // The whole enum is one scope, so a new variant cannot print a secret
    // either, whichever variant holds the redacted field.
    for (variants, message) in [
        (
            quote! {
                Apns { #[sdk(redact)] token: String },
                Fcm { token: String },
            },
            "`NotificationChannel::Fcm.token` is in a type with a redacted field; mark it #[sdk(redact)] or #[sdk(shown)]",
        ),
        (
            quote! {
                Fcm { token: String },
                Apns { #[sdk(redact)] token: String },
            },
            "`NotificationChannel::Fcm.token` is in a type with a redacted field; mark it #[sdk(redact)] or #[sdk(shown)]",
        ),
        (
            quote! {
                Apns { #[sdk(redact)] token: String },
                Http { #[sdk(redact)] url: String, signing_key: Vec<u8> },
            },
            "`NotificationChannel::Http.signing_key` is in a type with a redacted field; mark it #[sdk(redact)] or #[sdk(shown)]",
        ),
        // No option can mark a tuple variant field.
        (
            quote! {
                Apns { #[sdk(redact)] token: String },
                Fcm(String),
            },
            "`NotificationChannel` has a redacted field, so each of its fields needs a name to take #[sdk(redact)] or #[sdk(shown)]",
        ),
        (
            quote! {
                Apns { #[sdk(shown)] token: String },
                Fcm { #[sdk(shown)] token: String },
            },
            "#[sdk(shown)] applies in a type with a #[sdk(redact)] field",
        ),
    ] {
        let item = quote! {
            #[derive(Clone, uniffi::Enum)]
            pub enum NotificationChannel { #variants }
        };
        assert_eq!(error(quote!(), item), message);
    }
    // A variant without a redacted field shows its fields, and a unit
    // variant has none.
    let output = export(
        quote!(),
        quote! {
            #[derive(Clone, uniffi::Enum)]
            pub enum NotificationChannel {
                Apns { #[sdk(redact)] token: String },
                Webhook { #[sdk(shown)] name: String },
                Disabled,
            }
        },
    );
    assert!(output.contains("@xmtp-redact"));
}

// The macro implements Debug for a redacted type through the type's own
// `redacted_debug`. Any other Debug then conflicts with it, so a derived one
// fails to compile wherever it sits; tests/ui/sdk_export_redacted_debug.rs
// shows rustc's error.
#[test]
fn redacted_record_or_enum_gets_debug_from_redacted_debug() {
    let output = compact(&export(
        quote!(),
        quote! {
            /// A key.
            #[derive(Clone, uniffi::Record)]
            pub struct HmacKey {
                #[sdk(redact)]
                pub key: Vec<u8>,
                #[sdk(shown)]
                pub epoch: i64,
            }
        },
    ));
    assert!(output.contains(
        "#[doc=r\"Akey.\"]#[derive(Clone,uniffi::Record)]#[doc=\"@xmtp-redacted\"]pubstructHmacKey"
    ));
    assert!(output.ends_with(
        "impl::core::fmt::DebugforHmacKey{fnfmt(&self,formatter:&mut::core::fmt::Formatter<'_>)->::core::fmt::Result{Self::redacted_debug(self,formatter)}}"
    ));
    // The impl takes the item's target.
    let output = compact(&export(
        quote!(native_only),
        quote! {
            #[derive(Clone, uniffi::Enum)]
            pub enum NotificationChannel {
                Apns { #[sdk(redact)] token: String },
                Disabled,
            }
        },
    ));
    assert!(output.contains(
        "#[derive(Clone,uniffi::Enum)]#[doc=\"@xmtp-redacted\"]pubenumNotificationChannel"
    ));
    assert!(output.contains(
        "}#[cfg(not(target_arch=\"wasm32\"))]impl::core::fmt::DebugforNotificationChannel{"
    ));
    // So does a cfg written on the type.
    let output = compact(&export(
        quote!(),
        quote! {
            #[derive(Clone, uniffi::Record)]
            #[cfg(feature = "keys")]
            pub struct HmacKey { #[sdk(redact)] pub key: Vec<u8> }
        },
    ));
    assert!(output.contains("}#[cfg(feature=\"keys\")]impl::core::fmt::DebugforHmacKey{"));
    // A Debug derive that the macro sees gets a direct error; rustc rejects
    // any other one as a second impl.
    for derive in [quote!(Debug), quote!(std::fmt::Debug)] {
        let message = error(
            quote!(),
            quote! {
                #[derive(Clone, #derive, uniffi::Record)]
                pub struct HmacKey { #[sdk(redact)] pub key: Vec<u8> }
            },
        );
        assert_eq!(
            message,
            "`HmacKey` has a #[sdk(redact)] field, so sdk_export implements its Debug through `fn redacted_debug`; remove this derive"
        );
    }
    // Without a redacted field, the type keeps its own Debug.
    for item in [
        quote! {
            #[derive(Clone, Debug, uniffi::Record)]
            pub struct Plain { pub epoch: i64 }
        },
        quote! {
            #[derive(Clone, Debug, uniffi::Enum)]
            pub enum Channel { Apns { token: String } }
        },
    ] {
        let output = export(quote!(), item);
        assert!(!output.contains("@xmtp-"));
        assert!(!output.contains("impl"));
    }
}

// UniFFI's Kotlin binding renames an error type, so its diagnostics cannot be
// generated.
#[test]
fn uniffi_error_types_cannot_redact() {
    for derive in [
        quote!(#[derive(uniffi::Error)]),
        quote!(#[derive(Debug, thiserror::Error, uniffi::Error)]),
    ] {
        let message = error(
            quote!(),
            quote! {
                #derive
                pub enum CredentialError {
                    Rejected { #[sdk(redact)] token: String },
                }
            },
        );
        assert!(
            message.starts_with("`CredentialError` derives uniffi::Error, which the Kotlin binding renames to an exception class"),
            "{message}"
        );
    }
    // Another derive named Error is not UniFFI's.
    export(
        quote!(),
        quote! {
            #[derive(thiserror::Error, uniffi::Enum)]
            pub enum Channel {
                Apns { #[sdk(redact)] token: String },
            }
        },
    );
}

// The generator trusts the markers the macro writes. One written in a doc
// comment would skip the macro's checks, so the macro rejects it.
#[test]
fn markers_the_macro_writes_are_rejected_in_doc_comments() {
    for (item, option) in [
        (
            quote!(
                #[derive(uniffi::Record)]
                pub struct Session {
                    /// The token. @xmtp-redact
                    pub token: String,
                }
            ),
            "write #[sdk(redact)] instead of `@xmtp-redact`",
        ),
        (
            quote!(
                #[derive(uniffi::Record)]
                pub struct Session {
                    #[doc = "@xmtp-redact=secret"]
                    pub parameters: HashMap<String, String>,
                }
            ),
            "write #[sdk(redact)] instead of `@xmtp-redact`",
        ),
        (
            quote!(
                /// @xmtp-redacted
                #[derive(uniffi::Record)]
                pub struct Session {
                    pub token: String,
                }
            ),
            "write #[sdk(redact)] on a field instead of `@xmtp-redacted`",
        ),
        (
            quote!(
                #[derive(uniffi::Enum)]
                pub enum Channel {
                    Apns {
                        /// @xmtp-redact
                        token: String,
                    },
                }
            ),
            "write #[sdk(redact)] instead of `@xmtp-redact`",
        ),
        (
            quote!(
                #[derive(uniffi::Enum)]
                pub enum EventKind {
                    /// @xmtp-kind=lagged
                    Lagged,
                }
            ),
            "write #[sdk(kind = \"...\")] instead of `@xmtp-kind`",
        ),
        (
            quote!(impl Client {
                /// The ID. @xmtp-immutable
                pub fn id(&self) -> u64 { 0 }
            }),
            "write #[sdk(immutable)] instead of `@xmtp-immutable`",
        ),
        (
            quote!(
                trait Api {
                    /// @xmtp-immutable
                    fn id(&self) -> u64;
                }
            ),
            "write #[sdk(immutable)] instead of `@xmtp-immutable`",
        ),
        (
            quote!(
                /// @xmtp-pure
                pub fn encode() -> u64 {
                    0
                }
            ),
            "write #[sdk_export(pure)] instead of `@xmtp-pure`",
        ),
        (
            quote!(
                /// Reads the backend. @xmtp-client-static
                pub async fn inbox_states_with_backend() {}
            ),
            "write #[sdk_export(client_static)] instead of `@xmtp-client-static`",
        ),
        // cfg_attr expands before UniFFI reads the docstring.
        (
            quote!(
                #[derive(uniffi::Record)]
                pub struct Session {
                    #[cfg_attr(all(), doc = "@xmtp-redact")]
                    pub token: String,
                }
            ),
            "write #[sdk(redact)] instead of `@xmtp-redact`",
        ),
        (
            quote!(
                #[derive(uniffi::Record)]
                #[cfg_attr(feature = "x", cfg_attr(all(), doc = "@xmtp-redacted"))]
                pub struct Session {
                    pub token: String,
                }
            ),
            "write #[sdk(redact)] on a field instead of `@xmtp-redacted`",
        ),
    ] {
        let message = error(quote!(), item);
        assert_eq!(message, format!("{option} in a doc comment"));
    }
    // A doc value that is not a string literal may expand into a marker.
    for item in [
        quote!(
            #[derive(uniffi::Record)]
            pub struct Session {
                #[doc = concat!("@xmtp-red", "act")]
                pub token: String,
            }
        ),
        quote!(
            #[derive(uniffi::Record)]
            #[cfg_attr(feature = "x", cfg_attr(all(), doc = concat!("@xmtp-red", "acted")))]
            pub struct Session {
                pub token: String,
            }
        ),
        quote!(impl Client {
            #[doc = include_str!("id.md")]
            pub fn id(&self) -> u64 { 0 }
        }),
    ] {
        assert_eq!(
            error(quote!(), item),
            "write the doc comment as a string literal, which the macro checks for markers"
        );
    }
    // A literal that a `macro_rules!` caller passes through as an expression
    // arrives in an invisible group. syn reads the literal inside, so the
    // macro still checks it.
    let forwarded =
        |text: &str| proc_macro2::Group::new(proc_macro2::Delimiter::None, quote!(#text));
    let marker = forwarded("@xmtp-redact");
    assert_eq!(
        error(
            quote!(),
            quote!(
                #[derive(uniffi::Record)]
                pub struct Session {
                    #[doc = #marker]
                    pub token: String,
                }
            )
        ),
        "write #[sdk(redact)] instead of `@xmtp-redact` in a doc comment"
    );
    let prose = forwarded("The token.");
    export(
        quote!(),
        quote!(
            #[derive(uniffi::Record)]
            pub struct Session {
                #[doc = #prose]
                pub token: String,
            }
        ),
    );
    // The markers written by hand, and prose, stay.
    export(
        quote!(),
        quote!(impl Client {
            /// Made by the worker. @xmtp-worker @xmtp-internal See `@xmtp-redact`.
            pub fn id(&self) -> u64 { 0 }
        }),
    );
    // `pure` and `client_static` write their own markers after the check.
    assert!(
        export(
            quote!(pure),
            quote!(
                pub fn encode() -> u64 {
                    0
                }
            )
        )
        .contains("@xmtp-pure")
    );
    assert!(
        export(
            quote!(client_static),
            quote!(
                pub async fn inbox_states_with_backend() {}
            )
        )
        .contains("@xmtp-client-static")
    );
}

#[test]
fn host_internal_methods_emit_checked_and_projection_markers() {
    let output = export(
        quote!(),
        quote! {
            impl FourthReceiver {
                #[sdk(immutable, host_internal)]
                pub fn private_owner(&self) -> u64 { 1 }
            }
        },
    );
    assert!(output.contains("@xmtp-host-internal"));
    assert!(output.contains("@xmtp-internal"));
    assert!(output.contains("@xmtp-immutable"));
}

#[test]
fn host_internal_rejects_other_locations_duplicates_and_written_markers() {
    for item in [
        quote!(
            #[derive(uniffi::Record)]
            struct Record {
                #[sdk(host_internal)]
                value: u64,
            }
        ),
        quote!(
            #[derive(uniffi::Enum)]
            enum Kind {
                #[sdk(host_internal)]
                One,
            }
        ),
        quote!(
            trait Api {
                #[sdk(host_internal)]
                fn method(&self);
            }
        ),
        quote!(impl Object { #[sdk(host_internal)] pub fn create() {} }),
    ] {
        let message = error(quote!(), item);
        assert!(message.contains("applies to object methods"), "{message}");
    }
    assert!(
        error(
            quote!(),
            quote!(impl Object {
                #[sdk(host_internal, host_internal)] pub async fn method(&self) {}
            })
        )
        .contains("sdk option is repeated")
    );
    assert!(
        error(
            quote!(),
            quote!(impl Object {
                #[doc = "@xmtp-host-internal"] pub async fn method(&self) {}
            })
        )
        .contains("#[sdk(host_internal)]")
    );
    assert!(
        error(
            quote!(),
            quote!(
                #[sdk(host_internal)]
                fn method() {}
            )
        )
        .contains("free function")
    );
}

#[test]
fn stream_declaration_emits_checked_names() {
    let output = export(
        quote!(),
        quote! {
            impl FourthReceiver {
                #[sdk(stream(name = "consume", options = "MessageStreamOptions", owner = "private_owner"))]
                pub async fn selected_reader(&self, selection: Option<crate::MessageReaderOptions>) -> Result<Arc<MessageReader>, XmtpError> { todo!() }
            }
        },
    );
    assert!(output.contains("@xmtp-stream=consume:MessageStreamOptions:private_owner"));
    assert!(!output.contains("sdk (stream"));
}

#[test]
fn stream_parameters_reject_missing_repeated_unknown_and_unsafe_values() {
    for (parameters, expected) in [
        (
            quote!(name = "consume", options = "MessageStreamOptions"),
            "stream is missing owner",
        ),
        (
            quote!(
                name = "consume",
                options = "MessageStreamOptions",
                owner = "key",
                extra = "value"
            ),
            "unknown stream parameter",
        ),
        (
            quote!(
                name = "consume",
                options = "MessageStreamOptions",
                owner = "key",
                owner = "other"
            ),
            "stream parameter is repeated",
        ),
        (
            quote!(
                name = "consume();evil",
                options = "MessageStreamOptions",
                owner = "key"
            ),
            "ASCII identifier string",
        ),
        (
            quote!(
                name = "consume",
                options = "MessageStreamOptions",
                owner = concat!("key")
            ),
            "ASCII identifier string",
        ),
    ] {
        let item = quote!(impl Object {
            #[sdk(stream(#parameters))]
            pub async fn reader(&self, options: Option<Options>) -> Result<Arc<MessageReader>, XmtpError> { todo!() }
        });
        let message = error(quote!(), item);
        assert!(message.contains(expected), "{message}");
    }
    let message = error(
        quote!(),
        quote!(impl Object {
            #[sdk(stream(name = "one", options = "MessageStreamOptions", owner = "key"))]
            #[sdk(stream(name = "two", options = "MessageStreamOptions", owner = "key"))]
            pub async fn reader(&self, options: Option<Options>) -> Result<Arc<MessageReader>, XmtpError> { todo!() }
        }),
    );
    assert!(message.contains("sdk option is repeated"));
}

#[test]
fn stream_declaration_rejects_unsupported_signatures_and_locations() {
    for method in [
        quote!(
            pub fn reader(
                &self,
                options: Option<Options>,
            ) -> Result<Arc<MessageReader>, XmtpError> {
                todo!()
            }
        ),
        quote!(
            pub async fn reader(
                &mut self,
                options: Option<Options>,
            ) -> Result<Arc<MessageReader>, XmtpError> {
                todo!()
            }
        ),
        quote!(
            pub async fn reader(&self) -> Result<Arc<MessageReader>, XmtpError> {
                todo!()
            }
        ),
        quote!(
            pub async fn reader(&self, options: Options) -> Result<Arc<MessageReader>, XmtpError> {
                todo!()
            }
        ),
        quote!(
            pub async fn reader(&self, options: Option<Options>) -> Arc<MessageReader> {
                todo!()
            }
        ),
        quote!(
            pub async fn reader(
                &self,
                options: Option<Options>,
            ) -> Result<Arc<EventReader>, XmtpError> {
                todo!()
            }
        ),
        quote!(
            pub async fn reader(
                &self,
                options: Option<Options>,
            ) -> Result<Arc<MessageReader>, OtherError> {
                todo!()
            }
        ),
    ] {
        let message = error(
            quote!(),
            quote!(impl Object {
                #[sdk(stream(name = "consume", options = "MessageStreamOptions", owner = "key"))]
                #method
            }),
        );
        assert!(
            message.contains("needs an async object reader method"),
            "{message}"
        );
    }
    for item in [
        quote!(
            #[derive(uniffi::Record)]
            struct Record {
                #[sdk(stream(name = "consume", options = "MessageStreamOptions", owner = "key"))]
                value: u64,
            }
        ),
        quote!(
            #[derive(uniffi::Enum)]
            enum Kind {
                #[sdk(stream(name = "consume", options = "MessageStreamOptions", owner = "key"))]
                One,
            }
        ),
    ] {
        assert!(error(quote!(), item).contains("applies to object reader methods"));
    }
    assert!(
        error(
            quote!(),
            quote!(
                trait Api {
                    #[sdk(stream(
                        name = "consume",
                        options = "MessageStreamOptions",
                        owner = "key"
                    ))]
                    async fn reader(
                        &self,
                        options: Option<Options>,
                    ) -> Result<Arc<MessageReader>, XmtpError>;
                }
            )
        )
        .contains("needs an async object reader method")
    );
    assert!(
        error(
            quote!(),
            quote!(
                #[sdk(stream(name = "consume", options = "MessageStreamOptions", owner = "key"))]
                async fn reader(options: Option<Options>) -> Result<Arc<MessageReader>, XmtpError> {
                    todo!()
                }
            )
        )
        .contains("free function")
    );
    assert!(error(quote!(), quote!(impl Object {
        #[doc = "@xmtp-stream=consume:MessageStreamOptions:key"]
        pub async fn reader(&self, options: Option<Options>) -> Result<Arc<MessageReader>, XmtpError> { todo!() }
    })).contains("write #[sdk(stream(...))]"));
}
