extern crate proc_macro;

mod builder;

#[cfg(test)]
mod builder_test;

use quote::quote;

/// Attribute macro that generates a NAPI-annotated builder pattern for a struct.
///
/// Each field must be annotated with one of:
/// - `#[builder(required)]` — passed in the constructor; no setter generated
/// - `#[builder(optional)]` — field type must be `Option<T>`; setter takes `T`, wraps in `Some`
/// - `#[builder(default = "expr")]` — has a default value; setter takes the full type
/// - `#[builder(skip)]` — no setter; initialized via `Default::default()`
///
/// The macro generates a `new()` constructor (with all required fields as parameters)
/// and fluent setters for optional/default fields. The `build()` method is NOT generated;
/// implement it manually.
///
/// # Example
///
/// ```ignore
/// #[napi_builder]
/// pub struct FooBuilder {
///     #[builder(required)]
///     name: String,
///     #[builder(optional)]
///     desc: Option<String>,
///     #[builder(default = "42")]
///     count: u32,
///     #[builder(skip)]
///     internal: Vec<u8>,
/// }
/// ```
#[proc_macro_attribute]
pub fn napi_builder(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    fn napi_setter_ann(_ident: &syn::Ident) -> proc_macro2::TokenStream {
        quote! { #[::napi_derive::napi] }
    }

    let item = syn::parse_macro_input!(input as syn::ItemStruct);
    let config = builder::AnnotationConfig {
        struct_ann: quote! { #[::napi_derive::napi] },
        impl_ann: quote! { #[::napi_derive::napi] },
        constructor_ann: quote! { #[::napi_derive::napi(constructor)] },
        setter_ann: napi_setter_ann,
        setter_impl_ann: None,
        setter_style: builder::SetterStyle::NapiThis,
        setter_prefix: "set_",
    };
    match builder::expand_builder(item, config) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Attribute macro that generates a wasm_bindgen-annotated builder pattern for a struct.
///
/// Behaves identically to [`napi_builder`] but emits `#[wasm_bindgen]` annotations
/// instead of `#[napi]`, and generates `js_name = camelCase` attributes on setters.
///
/// See [`napi_builder`] for field attribute documentation.
#[proc_macro_attribute]
pub fn wasm_builder(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    fn wasm_setter_ann(ident: &syn::Ident) -> proc_macro2::TokenStream {
        let js_name = builder::to_camel_case(&ident.to_string());
        quote! { #[::wasm_bindgen::prelude::wasm_bindgen(js_name = #js_name)] }
    }

    let item = syn::parse_macro_input!(input as syn::ItemStruct);
    let config = builder::AnnotationConfig {
        struct_ann: quote! { #[::wasm_bindgen::prelude::wasm_bindgen(getter_with_clone)] },
        impl_ann: quote! { #[::wasm_bindgen::prelude::wasm_bindgen] },
        constructor_ann: quote! { #[::wasm_bindgen::prelude::wasm_bindgen(constructor)] },
        setter_ann: wasm_setter_ann,
        setter_impl_ann: None,
        setter_style: builder::SetterStyle::Consuming,
        setter_prefix: "set_",
    };
    match builder::expand_builder(item, config) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Attribute macro that generates a UniFFI-annotated builder pattern for a struct.
///
/// Emits `#[derive(uniffi::Object)]` on the struct and `#[uniffi::export]` on the
/// impl block. UniFFI annotates the impl block as a whole rather than individual
/// methods, so `constructor_ann` and `setter_ann` are empty.
///
/// See [`napi_builder`] for field attribute documentation.
#[proc_macro_attribute]
pub fn uniffi_builder(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    fn no_ann(_ident: &syn::Ident) -> proc_macro2::TokenStream {
        quote! {}
    }

    let item = syn::parse_macro_input!(input as syn::ItemStruct);
    let config = builder::AnnotationConfig {
        struct_ann: quote! { #[derive(::uniffi::Object)] },
        impl_ann: quote! { #[::uniffi::export] },
        constructor_ann: quote! { #[::uniffi::constructor] },
        setter_ann: no_ann,
        // UniFFI wraps objects in Arc<Self>, so `&mut self` setters can't
        // live inside `#[uniffi::export]`. Place them in a plain impl block.
        setter_impl_ann: Some(quote! {}),
        setter_style: builder::SetterStyle::MutRefChain,
        setter_prefix: "",
    };
    match builder::expand_builder(item, config) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
