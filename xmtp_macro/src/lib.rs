extern crate proc_macro;

use proc_macro2::*;
use quote::{quote, quote_spanned};

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

        quote! {
            #[cfg_attr(not(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none"))), tokio::test(flavor = #flavor))]
            #[cfg_attr(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")), wasm_bindgen_test::wasm_bindgen_test)]
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
                result.extend(quote!(#token));
            }
        }
    }

    result.into()
}

#[derive(Default)]
struct Attributes {
    r#async: bool,
    flavor: Option<syn::LitStr>,
    unwrap_try: Option<syn::LitStr>,
}

impl Attributes {
    fn flavor(&self) -> syn::LitStr {
        self.flavor
            .as_ref()
            .cloned()
            .unwrap_or(syn::LitStr::new("current_thread", Span::call_site()))
    }

    fn unwrap_try(&self) -> bool {
        self.unwrap_try
            .as_ref()
            .is_some_and(|v| v.value() == "true")
    }
}

impl Attributes {
    fn parse(&mut self, meta: &syn::meta::ParseNestedMeta) -> syn::Result<()> {
        if meta.path.is_ident("async") {
            self.r#async = true;
            return Ok(());
        } else if meta.path.is_ident("flavor") {
            self.flavor = Some(meta.value()?.parse()?);
            return Ok(());
        } else if meta.path.is_ident("unwrap_try") {
            self.unwrap_try = Some(meta.value()?.parse()?);
            return Ok(());
        }

        Err(meta.error("unknown attribute"))
    }
}
