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
            "unknown sdk option; expected immutable, kind",
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
    assert!(message.contains("unknown sdk option; expected immutable, kind"));
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
    assert_eq!(output.matches("@xmtp-").count(), 3);
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
            "#[sdk(shown)] applies beside a #[sdk(redact)] field",
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

// Redaction fails closed: next to a redacted field, every field says whether
// it is redacted or shown, so a new field cannot print a secret by default.
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
        "`Credential.expires_at_seconds` sits beside a redacted field; mark it #[sdk(redact)] or #[sdk(shown)]"
    );
    // Each variant is its own scope.
    let message = error(
        quote!(),
        quote! {
            #[derive(Clone, uniffi::Enum)]
            pub enum NotificationChannel {
                Apns { #[sdk(redact)] token: String },
                Fcm { token: String },
                Http { #[sdk(redact)] url: String, signing_key: Vec<u8> },
            }
        },
    );
    assert_eq!(
        message,
        "`NotificationChannel::Http.signing_key` sits beside a redacted field; mark it #[sdk(redact)] or #[sdk(shown)]"
    );
}

// A derived Debug prints every field, so a redacted record writes its own.
#[test]
fn redacted_record_or_enum_must_not_derive_debug() {
    let message = error(
        quote!(),
        quote! {
            #[derive(Clone, Debug, uniffi::Record)]
            pub struct HmacKey {
                #[sdk(redact)]
                pub key: Vec<u8>,
                #[sdk(shown)]
                pub epoch: i64,
            }
        },
    );
    assert_eq!(
        message,
        "`HmacKey` derives Debug, which prints `key`; write an `impl Debug` that redacts it. Keep sdk_export the first attribute, above every derive: it cannot see a derive written above it"
    );
    let message = error(
        quote!(),
        quote! {
            #[derive(Clone, std::fmt::Debug, uniffi::Enum)]
            pub enum NotificationChannel {
                Apns { #[sdk(redact)] token: String },
            }
        },
    );
    assert!(message.starts_with("`NotificationChannel` derives Debug, which prints `token`"));
    // UniFFI's Kotlin binding renames an error type, so its diagnostics
    // cannot be generated; the check comes before the Debug one.
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
    // Without a redacted field, a derived Debug is fine.
    export(
        quote!(),
        quote! {
            #[derive(Clone, Debug, uniffi::Record)]
            pub struct Plain {
                pub epoch: i64,
            }
        },
    );
}
