use crate::sdk_export::{callback_error, sdk_export};
use quote::quote;

fn export(attr: proc_macro2::TokenStream, item: proc_macro2::TokenStream) -> String {
    sdk_export(attr, item).unwrap().to_string()
}

#[test]
fn export_selects_runtime_and_instruments_every_impl_method() {
    let output = export(
        quote!(),
        quote! {
            impl Client {
                pub async fn sync(&self) -> Result<(), Error> { Ok(()) }
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

#[test]
fn export_limits_whole_item_to_selected_target() {
    let item = quote!(impl Client { pub fn count(&self) -> usize { 0 } });
    let native = export(quote!(native_only), item.clone());
    let wasm = export(quote!(wasm_only), item);

    assert!(native.starts_with("# [cfg (not (target_arch = \"wasm32\"))]"));
    assert!(wasm.starts_with("# [cfg (target_arch = \"wasm32\")]"));
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
    assert!(error.to_string().contains("native_only or wasm_only"));

    let error = sdk_export(
        quote!(),
        quote!(
            struct Client;
        ),
    )
    .unwrap_err();
    assert!(error.to_string().contains("impl block, trait, or function"));
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
