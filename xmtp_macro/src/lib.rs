extern crate proc_macro;

mod logging;

use proc_macro::TokenStream as TokenStream1;
use proc_macro2::*;
use quote::{quote, quote_spanned};
use syn::{Data, DeriveInput, parse_macro_input};

use crate::logging::*;

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
pub fn log_event_macro(_attr: TokenStream1, item: TokenStream1) -> TokenStream1 {
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
    let mut macro_arms = Vec::new();
    let mut cleaned_variants = Vec::new();

    for variant in &data_enum.variants {
        let variant_name = &variant.ident;
        let doc_comment = get_doc_comment(&variant.attrs);
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

        // Check if inbox_id is in context fields for message formatting
        let has_inbox_id = context_fields.contains(&"inbox_id".to_string());

        // Macro arm
        let full_path = format!("{}::{}", enum_name, variant_name);

        if context_fields.is_empty() {
            let arm = format!(
                r#"({} $(, $($extra:tt)*)?) => {{
        ::tracing::info!($($($extra)*)? {})
    }};"#,
                full_path,
                quote_string(&doc_comment),
            );
            macro_arms.push(arm);
        } else {
            let arms = generate_field_arms(&full_path, &context_fields, &doc_comment, has_inbox_id);
            macro_arms.extend(arms);
        }
    }

    // Build the macro_rules as a string and parse it
    let macro_body = macro_arms.join("\n        ");
    let macro_def_str = format!(
        r#"
macro_rules! log_event {{
    {}
}}

pub(crate) use log_event;
"#,
        macro_body
    );

    let macro_def: TokenStream = macro_def_str
        .parse()
        .expect("Failed to parse generated macro");

    let expanded = quote! {
        #(#attrs)*
        #visibility enum #enum_name {
            #(#cleaned_variants),*
        }

        impl ::core::fmt::Display for #enum_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#display_arms)*
                }
            }
        }

        #macro_def
    };

    expanded.into()
}

/// Derive macro for just the Display impl (if you don't need the log_event! macro)
#[proc_macro_derive(LogEvent, attributes(context))]
pub fn derive_log_event(input: TokenStream1) -> TokenStream1 {
    let input = parse_macro_input!(input as DeriveInput);

    let enum_name = &input.ident;

    let Data::Enum(data_enum) = &input.data else {
        return syn::Error::new_spanned(&input, "LogEvent can only be derived for enums")
            .to_compile_error()
            .into();
    };

    let mut display_arms = Vec::new();

    for variant in &data_enum.variants {
        let variant_name = &variant.ident;
        let doc_comment = get_doc_comment(&variant.attrs);

        display_arms.push(quote! {
            #enum_name::#variant_name => write!(f, #doc_comment),
        });
    }

    let expanded = quote! {
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
