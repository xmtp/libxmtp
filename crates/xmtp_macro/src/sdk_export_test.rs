use crate::sdk_export::{callback_error, sdk_export};
use quote::quote;

pub(crate) fn export(attr: proc_macro2::TokenStream, item: proc_macro2::TokenStream) -> String {
    sdk_export(attr, item).unwrap().to_string()
}

pub(crate) fn error(attr: proc_macro2::TokenStream, item: proc_macro2::TokenStream) -> String {
    sdk_export(attr, item).unwrap_err().to_string()
}

/// Token output without whitespace, so assertions do not depend on spacing.
pub(crate) fn compact(output: &str) -> String {
    output.split_whitespace().collect()
}

#[test]
fn export_selects_runtime_and_instruments_every_impl_method() {
    let output = export(
        quote!(),
        quote! {
            impl Client {
                pub async fn sync(&self) -> Result<(), Error> { Ok(()) }
                #[sdk(immutable)]
                pub fn count(&self) -> usize { 0 }
                fn helper(&self) -> Result<(), Error> { Ok(()) }
            }
        },
    );

    assert!(output.contains(
        "cfg_attr (not (target_arch = \"wasm32\") , uniffi :: export (async_runtime = \"tokio\"))"
    ));
    assert!(output.contains("cfg_attr (target_arch = \"wasm32\" , uniffi :: export)"));
    assert!(output.contains("tracing :: instrument (err , skip_all)"));
    assert!(output.contains("tracing :: instrument (skip_all)"));
    assert_eq!(output.matches("tracing :: instrument").count(), 3);
}

#[test]
fn sync_only_impl_uses_plain_export() {
    let output = export(
        quote!(),
        quote! {
            impl Client {
                #[uniffi::constructor]
                pub fn new() -> Self { Self }
                #[sdk(immutable)]
                pub fn count(&self) -> usize { 0 }
            }
        },
    );

    assert!(output.contains("# [uniffi :: export]"));
    assert!(!output.contains("async_runtime"));
    assert_eq!(output.matches("tracing :: instrument").count(), 2);
}

#[test]
fn sync_only_trait_impl_instruments_inherited_visibility_methods() {
    let output = export(
        quote!(),
        quote! {
            impl ClientApi for Client {
                #[sdk(immutable)]
                fn count(&self) -> usize { 0 }
                fn check(&self) -> Result<(), Error> { Ok(()) }
            }
        },
    );

    assert!(output.contains("# [uniffi :: export]"));
    assert!(!output.contains("async_runtime"));
    assert!(output.contains("tracing :: instrument (err , skip_all)"));
    assert_eq!(output.matches("tracing :: instrument").count(), 2);
}

#[test]
fn export_keeps_existing_instrument() {
    let output = export(
        quote!(),
        quote! {
            impl Client {
                #[tracing::instrument(level = "debug")]
                pub fn sync(&self) -> Result<(), Error> { Ok(()) }
            }
        },
    );

    assert!(output.contains("tracing :: instrument (level = \"debug\")"));
    assert!(!output.contains("tracing :: instrument (err , skip_all)"));
}

// A target argument is the cfg that it replaces, and nothing else.
#[test]
fn export_limits_whole_item_to_selected_target() {
    let item = quote!(impl Client {
        pub async fn sync(&self) -> Result<(), Error> { Ok(()) }
        #[sdk(immutable)]
        pub fn count(&self) -> usize { 0 }
    });
    let native = compact(&export(quote!(native_only), item.clone()));
    let wasm = compact(&export(quote!(wasm_only), item));

    assert!(native.starts_with("#[cfg(not(target_arch=\"wasm32\"))]"));
    assert!(wasm.starts_with("#[cfg(target_arch=\"wasm32\")]"));
    for output in [native, wasm] {
        assert_eq!(output.matches("#[cfg(").count(), 1);
        assert_eq!(output.matches("@xmtp-").count(), 1);
        assert!(output.contains("#[doc=\"@xmtp-immutable\"]"));
    }

    let function = compact(&export(
        quote!(native_only),
        quote!(
            pub async fn suspend_streams() -> Result<(), Error> {
                Ok(())
            }
        ),
    ));
    assert!(function.starts_with("#[cfg(not(target_arch=\"wasm32\"))]"));
    assert!(!function.contains("@xmtp-"));
}

#[test]
fn export_accepts_free_function() {
    let output = export(
        quote!(),
        quote!(
            pub async fn run() -> std::result::Result<(), Error> {
                Ok(())
            }
        ),
    );

    assert!(output.contains("tracing :: instrument (err , skip_all)"));
    assert!(output.contains("async fn run"));
    assert!(output.contains("async_runtime = \"tokio\""));
    assert!(!output.contains("@xmtp-"));
}

#[test]
fn sync_free_function_uses_plain_export() {
    let output = export(
        quote!(),
        quote!(
            pub fn count() -> usize {
                0
            }
        ),
    );

    assert!(output.contains("# [uniffi :: export]"));
    assert!(!output.contains("async_runtime"));
    assert!(output.contains("tracing :: instrument (skip_all)"));
}

#[test]
fn pure_free_function_has_metadata_marker() {
    let output = export(
        quote!(pure),
        quote!(
            pub fn version() -> String {
                String::new()
            }
        ),
    );
    assert!(output.contains("@xmtp-pure"));
    assert!(compact(&output).contains(
        "#[cfg_attr(any(not(target_arch=\"wasm32\"),feature=\"pure-only\"),uniffi::export)]"
    ));
    assert!(!output.contains("# [uniffi :: export]"));
    assert!(!output.contains("async_runtime"));
}

#[test]
fn pure_rejects_async_and_methods() {
    let error = sdk_export(
        quote!(pure),
        quote!(
            pub async fn invalid() {}
        ),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("pure export must be synchronous")
    );
    let error = sdk_export(
        quote!(pure),
        quote!(impl Client { pub fn invalid(&self) {} }),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("pure export must be a free function")
    );
}

#[test]
fn async_trait_selects_runtime() {
    let output = export(
        quote!(),
        quote! {
            trait ClientApi {
                async fn sync(&self) -> Result<(), Error>;
            }
        },
    );

    assert!(output.contains("async_runtime = \"tokio\""));
}

#[test]
fn export_rejects_unknown_argument_and_wrong_item() {
    let item = quote!(impl Client {});
    let error = sdk_export(quote!(other), item).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("native_only, wasm_only, pure, and default(...)")
    );

    let error = sdk_export(
        quote!(),
        quote!(
            const ANSWER: u32 = 42;
        ),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("impl block, trait, function, record, or enum")
    );
}

#[test]
fn export_rejects_conflicting_arguments() {
    let function = quote!(
        pub fn version() -> String {
            String::new()
        }
    );
    assert!(
        error(quote!(native_only, wasm_only), function.clone()).contains("more than one target")
    );
    assert!(error(quote!(pure, pure), function.clone()).contains("more than one pure"));
    assert!(
        error(quote!(pure, native_only), function).contains("pure export selects its own targets")
    );
}

#[test]
fn callback_error_asserts_conversion_for_struct_and_enum() {
    for item in [
        quote!(
            struct Error;
        ),
        quote!(
            enum Error {
                Failed,
            }
        ),
    ] {
        let output = callback_error(quote!(), item).unwrap().to_string();
        assert!(output.contains("assert_from :: < Error > ()"));
        assert!(output.contains("uniffi :: UnexpectedUniFFICallbackError"));
    }
}

#[test]
fn callback_error_rejects_generics_and_wrong_item() {
    let error = callback_error(
        quote!(),
        quote!(
            struct Error<T>(T);
        ),
    )
    .unwrap_err();
    assert!(error.to_string().contains("concrete type"));

    let error = callback_error(
        quote!(),
        quote!(
            fn error() {}
        ),
    )
    .unwrap_err();
    assert!(error.to_string().contains("enum or struct"));
}

#[test]
fn pure_default_is_forwarded_to_the_stock_export() {
    let output = export(
        quote!(pure, default(nonce = None)),
        quote!(
            pub fn inbox(nonce: Option<u64>) -> String {
                String::new()
            }
        ),
    );
    assert!(output.contains("@xmtp-pure"));
    assert!(output.contains("uniffi :: export (default (nonce = None))"));
    assert!(
        sdk_export(
            quote!(native_only, default(nonce = None)),
            quote!(
                pub fn inbox(nonce: Option<u64>) {}
            )
        )
        .is_err()
    );
}

// Targets: a record or enum that exists on one target gets the cfg, and a
// field keeps its own cfg.

#[test]
fn target_limited_record_and_enum_emit_cfg() {
    let record = compact(&export(
        quote!(native_only),
        quote!(
            #[derive(uniffi::Record)]
            pub struct NotificationConfig {
                pub channel: NotificationChannel,
                #[cfg(not(target_arch = "wasm32"))]
                pub key: Option<Vec<u8>>,
            }
        ),
    ));
    assert!(record.starts_with(
        "#[cfg(not(target_arch=\"wasm32\"))]#[derive(uniffi::Record)]pubstructNotificationConfig"
    ));
    assert!(record.contains("#[cfg(not(target_arch=\"wasm32\"))]pubkey"));
    let enumeration = compact(&export(
        quote!(wasm_only),
        quote!(
            #[derive(uniffi::Enum)]
            pub enum WorkerState {
                Idle,
                Busy,
            }
        ),
    ));
    assert!(
        enumeration
            .starts_with("#[cfg(target_arch=\"wasm32\")]#[derive(uniffi::Enum)]pubenumWorkerState")
    );
    assert!(!format!("{record}{enumeration}").contains("@xmtp-"));
}

#[test]
fn record_export_requires_the_uniffi_derive_after_it() {
    for item in [
        quote!(
            pub struct Options {
                pub key: String,
            }
        ),
        quote!(
            #[derive(Clone, Debug)]
            pub struct Options {
                pub key: String,
            }
        ),
        quote!(
            pub enum EventKind {
                #[sdk(kind = "lagged")]
                Lagged,
            }
        ),
    ] {
        let message = error(quote!(), item);
        assert!(
            message.contains("sdk_export must be the first attribute on a record or enum"),
            "{message}"
        );
    }
    let record = quote!(
        #[derive(uniffi::Record)]
        pub struct Key {
            pub bytes: Vec<u8>,
        }
    );
    assert!(error(quote!(pure), record).contains("pure export must be a free function"));
}
