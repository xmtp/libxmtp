extern crate proc_macro;

use proc_macro2::*;
use quote::{quote, quote_spanned};
use syn::{Data, DeriveInput, Fields};

/// A proc macro attribute that wraps the input in an `async_trait` implementation,
/// delegating to the appropriate `async_trait` implementation based on the target architecture.
///
/// On wasm32 architecture, it delegates to `async_trait::async_trait(?Send)`.
/// On all other architectures, it delegates to `async_trait::async_trait`.
#[proc_macro_attribute]
pub fn async_trait(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::Item);
    quote! {
        #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
        #input
    }
    .into()
}

// This needs to be configurable here, because we can't look at env variables in wasm
static DISABLE_LOGGING: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    std::env::var("CI").is_ok_and(|v| v == "true")
        || std::env::var("XMTP_TEST_LOGGING").is_ok_and(|v| v == "false")
});

/// A test macro that delegates to the appropriate test framework based on the target architecture.
///
/// On wasm32 architecture, it delegates to `wasm_bindgen_test::wasm_bindgen_test`.
/// On all other architectures, it delegates to `tokio::test`.
///
/// When using with 'rstest', ensure any other test invocations come after rstest invocation.
/// # Example
///
/// ```ignore
/// #[test]
/// async fn test_something() {
///     assert_eq!(2 + 2, 4);
/// }
/// ```
#[proc_macro_attribute]
pub fn test(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    // Parse the input function attributes
    let mut attributes = Attributes::default();
    let attribute_parser = syn::meta::parser(|meta| attributes.parse(&meta));
    syn::parse_macro_input!(attr with attribute_parser);

    // Parse the function as an ItemFn
    let mut input_fn = syn::parse_macro_input!(body as syn::ItemFn);
    let is_async = input_fn.sig.asyncness.is_some();

    // Generate the appropriate test attributes
    let test_attrs = if is_async {
        let flavor = attributes.flavor();

        if &flavor.value() != "current_thread" {
            let workers = attributes.worker_threads();
            quote! {
                #[cfg_attr(not(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none"))), tokio::test(flavor = #flavor, worker_threads = #workers))]
                #[cfg_attr(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")), wasm_bindgen_test::wasm_bindgen_test)]
            }
        } else {
            quote! {
                #[cfg_attr(not(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none"))), tokio::test(flavor = #flavor))]
                #[cfg_attr(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")), wasm_bindgen_test::wasm_bindgen_test)]
            }
        }
    } else {
        quote! {
            #[cfg_attr(not(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none"))), test)]
            #[cfg_attr(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")), wasm_bindgen_test::wasm_bindgen_test)]
        }
    };

    // Transform ? to .unwrap() on functions that return ()
    let should_transform = attributes.unwrap_try() && returns_unit(&input_fn.sig.output);
    if should_transform {
        let input_fn_tokens = quote!(#input_fn);
        let transformed_tokens = transform_question_marks(input_fn_tokens.into());
        input_fn = syn::parse_macro_input!(transformed_tokens as syn::ItemFn);
    }

    let disable_logging = attributes.disable_logging || *DISABLE_LOGGING;
    if !disable_logging {
        let init = syn::parse_quote!(xmtp_common::logger(););
        input_fn.block.stmts.insert(0, init);
    }

    proc_macro::TokenStream::from(quote! {
        #test_attrs
        #input_fn
    })
}

// Check if a function's return type is () (unit)
fn returns_unit(return_type: &syn::ReturnType) -> bool {
    match return_type {
        // No explicit return type means it returns ()
        syn::ReturnType::Default => true,

        // Explicit return type, check if it's ()
        syn::ReturnType::Type(_, ty) => {
            if let syn::Type::Tuple(tuple) = &**ty {
                // Empty tuple () is the unit type
                tuple.elems.is_empty()
            } else {
                false
            }
        }
    }
}

// Transform ? operators to .unwrap() calls at the token level
fn transform_question_marks(tokens: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut result = proc_macro2::TokenStream::new();
    let tokens = proc_macro2::TokenStream::from(tokens)
        .into_iter()
        .peekable();

    for token in tokens {
        match &token {
            proc_macro2::TokenTree::Punct(p) if p.as_char() == '?' => {
                // Get the span from the question mark token
                let span = p.span();

                // Use quote_spanned! to generate .unwrap() with the original span
                let unwrap_tokens = quote_spanned! {span=>
                    .unwrap()
                };

                result.extend(unwrap_tokens);
            }
            proc_macro2::TokenTree::Group(g) => {
                // Recursively transform tokens in groups
                let transformed_stream = transform_question_marks(g.stream().into());

                let mut transformed_group = proc_macro2::Group::new(
                    g.delimiter(),
                    proc_macro2::TokenStream::from(transformed_stream),
                );

                // Preserve the span
                let span = g.span();
                transformed_group.set_span(span);
                result.extend(quote!(#transformed_group));
            }
            _ => {
                // Keep other tokens as is
                result.extend([token]);
            }
        }
    }

    result.into()
}

#[derive(Default)]
struct Attributes {
    flavor: Option<syn::LitStr>,
    worker_threads: Option<syn::LitInt>,
    unwrap_try: Option<bool>,
    disable_logging: bool,
}

impl Attributes {
    fn flavor(&self) -> syn::LitStr {
        self.flavor
            .as_ref()
            .cloned()
            .unwrap_or(syn::LitStr::new("current_thread", Span::call_site()))
    }

    fn unwrap_try(&self) -> bool {
        self.unwrap_try.as_ref().is_some_and(|v| *v)
    }

    fn worker_threads(&self) -> syn::LitInt {
        self.worker_threads
            .as_ref()
            .cloned()
            .unwrap_or(syn::LitInt::new(
                &num_cpus::get().to_string(),
                Span::call_site(),
            ))
    }
}

impl Attributes {
    fn parse(&mut self, meta: &syn::meta::ParseNestedMeta) -> syn::Result<()> {
        if meta.path.is_ident("flavor") {
            self.flavor = Some(meta.value()?.parse()?);
            return Ok(());
        } else if meta.path.is_ident("worker_threads") {
            self.worker_threads = Some(meta.value()?.parse()?);
            return Ok(());
        } else if meta.path.is_ident("unwrap_try") {
            self.unwrap_try = Some(meta.value()?.parse::<syn::LitBool>()?.value());
            return Ok(());
        } else if meta.path.is_ident("disable_logging") {
            self.disable_logging = meta.value()?.parse::<syn::LitBool>()?.value();
            return Ok(());
        }

        Err(meta.error("unknown attribute"))
    }
}

/// Derive macro for the `ErrorCode` trait.
///
/// Automatically generates an `error_code()` implementation that returns
/// `"{TypeName}::{VariantName}"` for each enum variant.
///
/// # Example
///
/// ```ignore
/// use xmtp_common::ErrorCode;
///
/// #[derive(Debug, thiserror::Error, ErrorCode)]
/// pub enum GroupError {
///     #[error("Group not found")]
///     NotFound,  // Returns "GroupError::NotFound"
///
///     #[error("Storage error: {0}")]
///     #[error_code(inherit)]  // Delegates to StorageError::error_code()
///     Storage(#[from] StorageError),
/// }
/// ```
///
/// # Attributes
///
/// - `#[error_code(inherit)]` - Delegate to the inner error's `error_code()` method.
///   Use this for single-field variants that wrap another error implementing `ErrorCode`.
///
/// - `#[error_code("CustomCode")]` - Override the generated code with a custom value.
///   Use this to maintain backwards compatibility when renaming variants.
///
/// # Example: Custom Code for Backwards Compatibility
///
/// ```ignore
/// #[derive(Debug, thiserror::Error, ErrorCode)]
/// pub enum MyError {
///     // Renamed from "OldName" but keeps the old error code
///     #[error("new name")]
///     #[error_code("MyError::OldName")]
///     NewName,
/// }
/// ```
// Parsed error_code attribute options
#[derive(Default)]
struct ErrorCodeAttr {
    /// Custom code override: #[error_code("CustomCode")]
    code: Option<String>,
    /// Inherit from inner error: #[error_code(inherit)]
    inherit: bool,
}

impl ErrorCodeAttr {
    fn parse(attrs: &[syn::Attribute]) -> Self {
        let mut result = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("error_code") {
                continue;
            }

            // Try parsing #[error_code("CustomCode")]
            if let Ok(lit) = attr.parse_args::<syn::LitStr>() {
                result.code = Some(lit.value());
                continue;
            }

            // Try parsing #[error_code(inherit)]
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("inherit") {
                    result.inherit = true;
                    Ok(())
                } else {
                    Err(meta.error("expected `inherit` or a string literal"))
                }
            });
        }

        result
    }
}

#[proc_macro_derive(ErrorCode, attributes(error_code))]
pub fn derive_error_code(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let name_str = name.to_string();

    let expanded = match &input.data {
        Data::Enum(data_enum) => {
            let code_arms = data_enum.variants.iter().map(|variant| {
                let variant_name = &variant.ident;
                let default_code = format!("{}::{}", name_str, variant_name);
                let attr = ErrorCodeAttr::parse(&variant.attrs);

                if attr.inherit {
                    // For inherited errors, delegate to the inner error
                    match &variant.fields {
                        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                            quote! {
                                Self::#variant_name(e) => e.error_code(),
                            }
                        }
                        Fields::Named(fields) if fields.named.len() == 1 => {
                            let field_name = fields.named.first().unwrap().ident.as_ref().unwrap();
                            quote! {
                                Self::#variant_name { #field_name } => #field_name.error_code(),
                            }
                        }
                        _ => {
                            let span = variant_name.span();
                            quote_spanned! {span=>
                                compile_error!("#[error_code(inherit)] requires exactly one field");
                            }
                        }
                    }
                } else {
                    // Use custom code if provided, otherwise use default
                    let code = attr.code.unwrap_or(default_code);

                    // Generate pattern based on fields
                    match &variant.fields {
                        Fields::Unit => {
                            quote! {
                                Self::#variant_name => #code,
                            }
                        }
                        Fields::Unnamed(_) => {
                            quote! {
                                Self::#variant_name(..) => #code,
                            }
                        }
                        Fields::Named(_) => {
                            quote! {
                                Self::#variant_name { .. } => #code,
                            }
                        }
                    }
                }
            });

            quote! {
                impl xmtp_common::ErrorCode for #name {
                    fn error_code(&self) -> &'static str {
                        match self {
                            #(#code_arms)*
                        }
                    }
                }
            }
        }
        Data::Struct(_) => {
            // Check for custom code on struct
            let attr = ErrorCodeAttr::parse(&input.attrs);
            let code = attr.code.unwrap_or_else(|| name_str.clone());

            quote! {
                impl xmtp_common::ErrorCode for #name {
                    fn error_code(&self) -> &'static str {
                        #code
                    }
                }
            }
        }
        Data::Union(_) => {
            return syn::Error::new_spanned(&input, "ErrorCode cannot be derived for unions")
                .to_compile_error()
                .into();
        }
    };

    expanded.into()
}
