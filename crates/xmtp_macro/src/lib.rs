extern crate proc_macro;

mod logging;

use proc_macro2::*;
use quote::{quote, quote_spanned};
use syn::{Data, DeriveInput, Fields, Path, parse_macro_input};

use crate::logging::{LogEventInput, get_context_fields, get_doc_comment, get_icon};

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

#[proc_macro_attribute]
pub fn build_logging_metadata(
    _attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    let enum_name = &input.ident;
    let visibility = &input.vis;
    let attrs = &input.attrs;

    let Data::Enum(data_enum) = &input.data else {
        return syn::Error::new_spanned(&input, "log_event_macro can only be used on enums")
            .to_compile_error()
            .into();
    };

    let mut display_arms = Vec::new();
    let mut metadata_entries = Vec::new();
    let mut cleaned_variants = Vec::new();
    let mut metadata_match_arms = Vec::new();

    for variant in &data_enum.variants {
        let variant_name = &variant.ident;
        let variant_name_str = variant_name.to_string();
        let doc_comment = match get_doc_comment(variant) {
            Ok(dc) => dc,
            Err(err) => return err.to_compile_error().into(),
        };
        let icon = get_icon(&variant.attrs).unwrap_or_default();
        let context_fields = get_context_fields(&variant.attrs);

        // Filter out #[context(...)] attributes for the output enum
        let filtered_attrs: Vec<_> = variant
            .attrs
            .iter()
            .filter(|a| !a.path().is_ident("context"))
            .collect();

        // Rebuild variant without context attribute
        let variant_fields = &variant.fields;
        let variant_discriminant = variant
            .discriminant
            .as_ref()
            .map(|(eq, expr)| quote! { #eq #expr });

        cleaned_variants.push(quote! {
            #(#filtered_attrs)*
            #variant_name #variant_fields #variant_discriminant
        });

        // Display impl arm
        display_arms.push(quote! {
            #enum_name::#variant_name => write!(f, #doc_comment),
        });

        // Metadata entry for the const array
        let context_fields_tokens: Vec<_> = context_fields.iter().map(|f| quote! { #f }).collect();
        metadata_entries.push(quote! {
            crate::EventMetadata {
                name: #variant_name_str,
                event: #enum_name::#variant_name,
                doc: #doc_comment,
                context_fields: &[#(#context_fields_tokens),*],
                icon: #icon,
            }
        });

        // Match arm for the metadata() method
        metadata_match_arms.push(quote! {
            #enum_name::#variant_name => &Self::METADATA[#enum_name::#variant_name as usize],
        });
    }

    let variant_count = cleaned_variants.len();

    let expanded = quote! {
        #(#attrs)*
        #[repr(usize)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #visibility enum #enum_name {
            #(#cleaned_variants),*
        }

        impl #enum_name {
            /// Metadata for all variants of this enum, indexed by variant discriminant.
            pub const METADATA: [crate::EventMetadata; #variant_count] = [
                #(#metadata_entries),*
            ];

            /// Returns the metadata for this event variant.
            pub const fn metadata(&self) -> &'static crate::EventMetadata {
                match self {
                    #(#metadata_match_arms)*
                }
            }
        }

        impl ::core::fmt::Display for #enum_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#display_arms)*
                }
            }
        }
    };

    expanded.into()
}

#[proc_macro]
pub fn log_event(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as LogEventInput);
    let event = &input.event;
    let installation_id = &input.installation_id;

    let provided_names: Vec<String> = input.fields.iter().map(|f| f.name.to_string()).collect();
    let tracing_fields: Vec<TokenStream> =
        input.fields.iter().map(|f| f.to_tracing_tokens()).collect();

    // Generate match arms for building context string (non-structured logging only)
    let context_match_arms: Vec<TokenStream> = input
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let name_str = &provided_names[i];
            let value = f.value_tokens();
            if matches!(f.sigil, Some('#')) {
                // # sigil (short_hex): always quote the value so hex strings
                // like "1713e608" aren't misinterpreted as scientific notation
                quote! {
                    #name_str => Some(format!("{}: \"{}\"", #name_str, #value))
                }
            } else if matches!(f.sigil, Some('$')) {
                // $ sigil (json): value is already a JSON string from serde_json::to_string
                quote! {
                    #name_str => Some(format!("{}: {}", #name_str, #value))
                }
            } else if matches!(f.sigil, Some('%')) {
                quote! {
                    #name_str => Some(format!("{}: {}", #name_str, #value))
                }
            } else {
                quote! {
                    #name_str => Some(format!("{}: {:?}", #name_str, #value))
                }
            }
        })
        .collect();

    let provided_names_tokens = provided_names.into_iter().map(|n| quote! { #n });

    // Generate the appropriate tracing level
    let level = match input.level {
        logging::LogLevel::Info => quote! { ::tracing::Level::INFO },
        logging::LogLevel::Warn => quote! { ::tracing::Level::WARN },
        logging::LogLevel::Error => quote! { ::tracing::Level::ERROR },
    };

    let tracing_call = quote! {
        ::tracing::event!(
            #level,
            #(#tracing_fields,)*
            "{}",
            __message
        );
    };

    quote! {
        {
            const PROVIDED: &[&str] = &[#(#provided_names_tokens),*];

            // Compile-time validation: ensure all required context fields are provided
            const _: () = #event.metadata().validate_fields(PROVIDED);

            let __meta = #event.metadata();

            // Bind installation_id to a variable to extend its lifetime
            let __installation_id = #installation_id;
            let __inst = xmtp_common::fmt::short_hex(__installation_id.as_ref());
            let __now_ms = xmtp_common::time::now_ms();

            // Build message with context for non-structured logging
            let __message = if ::xmtp_common::is_structured_logging() {
                // Structured logging: include installation_id and timestamp in message
                format!("➣ {} {{time_ms: {__now_ms}, inst: \"{__inst}\"}}", __meta.doc)
            } else {
                // Non-structured logging: embed context in message for readability
                let mut __context_parts: ::std::vec::Vec<String> = __meta.context_fields
                    .iter()
                    .filter_map(|&field_name| {
                        match field_name {
                            #(#context_match_arms,)*
                            _ => None,
                        }
                    })
                    .collect();

                __context_parts.push(format!("time_ms: {__now_ms}"));
                __context_parts.push(format!("inst: \"{__inst}\""));
                let __context_str = __context_parts.join(", ").replace('\n', " ");

                format!("➣ {} {{{__context_str}}}", __meta.doc)
            };

            #tracing_call
        }
    }
    .into()
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
/// `"TypeName::VariantName"` for each enum variant, or `"TypeName"` for structs.
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
/// - `#[error_code(remote = "path::Type")]` - Implement `ErrorCode` for a remote type.
///   The derived item should mirror the remote type's shape. Default codes use the derived
///   item's type name, so keep it aligned with the remote type's name unless overridden.
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
#[derive(Default)]
struct ErrorCodeAttr {
    /// Custom code override: #[error_code("CustomCode")]
    code: Option<String>,
    /// Inherit from inner error: #[error_code(inherit)]
    inherit: bool,
    /// Implement for a remote type path: #[error_code(remote = "path::Type")]
    remote: Option<Path>,
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

            // Try parsing #[error_code(inherit)] or #[error_code(remote = "path::Type")]
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("inherit") {
                    result.inherit = true;
                    Ok(())
                } else if meta.path.is_ident("remote") {
                    let value = meta.value()?;
                    let lit: syn::LitStr = value.parse()?;
                    let path = lit
                        .parse::<Path>()
                        .map_err(|err| meta.error(err.to_string()))?;
                    result.remote = Some(path);
                    Ok(())
                } else {
                    Err(meta.error(
                        "expected `inherit`, `remote = \"path::Type\"`, or a string literal",
                    ))
                }
            });
        }

        result
    }
}

#[proc_macro_derive(ErrorCode, attributes(error_code))]
pub fn derive_error_code(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let container_attr = ErrorCodeAttr::parse(&input.attrs);
    let name_str = container_attr
        .remote
        .as_ref()
        .and_then(|path| {
            path.segments
                .last()
                .map(|segment| segment.ident.to_string())
        })
        .unwrap_or_else(|| name.to_string());
    let target = container_attr
        .remote
        .clone()
        .unwrap_or_else(|| syn::parse_quote!(#name));

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
                impl xmtp_common::ErrorCode for #target {
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
            let code = container_attr.code.unwrap_or_else(|| name_str.clone());

            quote! {
                impl xmtp_common::ErrorCode for #target {
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
