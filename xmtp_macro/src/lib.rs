extern crate proc_macro;

mod logging;

use proc_macro2::*;
use quote::{quote, quote_spanned};
use syn::{Data, DeriveInput, parse_macro_input};

use crate::logging::{LogEventInput, get_context_fields, get_doc_comment};

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
        let doc_comment = match get_doc_comment(&variant) {
            Ok(dc) => dc,
            Err(err) => return err.to_compile_error().into(),
        };
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
                doc: #doc_comment,
                context_fields: &[#(#context_fields_tokens),*],
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

    let provided_names: Vec<String> = input.fields.iter().map(|f| f.name.to_string()).collect();
    let tracing_fields: Vec<TokenStream> =
        input.fields.iter().map(|f| f.to_tracing_tokens()).collect();

    // Generate match arms for building context string (non-structured logging only)
    let context_match_arms: Vec<TokenStream> = input
        .fields
        .iter()
        .map(|f| {
            let name_str = f.name.to_string();
            let value = f.value_tokens();
            quote! {
                #name_str => Some(format!("{}: {:?}", #name_str, #value))
            }
        })
        .collect();

    let provided_names_tokens = provided_names.iter().map(|n| quote! { #n });

    quote! {
        {
            const PROVIDED: &[&str] = &[#(#provided_names_tokens),*];

            // Compile-time validation: ensure all required context fields are provided
            const _: () = {
                const fn str_eq(a: &str, b: &str) -> bool {
                    let a = a.as_bytes();
                    let b = b.as_bytes();
                    if a.len() != b.len() {
                        return false;
                    }
                    let mut i = 0;
                    while i < a.len() {
                        if a[i] != b[i] {
                            return false;
                        }
                        i += 1;
                    }
                    true
                }

                let meta = #event.metadata();
                let mut i = 0;
                while i < meta.context_fields.len() {
                    let required = meta.context_fields[i];
                    let mut found = false;
                    let mut j = 0;
                    while j < PROVIDED.len() {
                        if str_eq(required, PROVIDED[j]) {
                            found = true;
                            break;
                        }
                        j += 1;
                    }
                    assert!(found, "log_event! missing required context field");
                    i += 1;
                }
            };

            let __meta = #event.metadata();

            // Build message with context for non-structured logging
            let __message = if ::xmtp_common::is_structured_logging() {
                // Structured logging: fields are already in JSON, don't duplicate in message
                __meta.doc.to_string()
            } else {
                // Non-structured logging: embed context in message for readability
                let __context_parts: ::std::vec::Vec<String> = __meta.context_fields
                    .iter()
                    .filter_map(|&field_name| {
                        match field_name {
                            #(#context_match_arms,)*
                            _ => None,
                        }
                    })
                    .collect();

                let __context_str = __context_parts.join(", ");
                if __context_str.is_empty() {
                    __meta.doc.to_string()
                } else {
                    format!("{} {{{}}}", __meta.doc, __context_str)
                }
            };

            ::tracing::info!(
                #(#tracing_fields,)*
                "{}",
                __message
            );
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
