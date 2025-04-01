extern crate proc_macro;

use proc_macro2::*;
use quote::quote;

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
    // Parse the input function
    let mut attributes = Attributes::default();
    let attribute_parser = syn::meta::parser(|meta| attributes.parse(meta));
    syn::parse_macro_input!(attr with attribute_parser);

    // Parse the function
    let mut input_fn = syn::parse_macro_input!(body as syn::ItemFn);

    // Check if function returns unit type () and if so, transform ? to unwrap()
    if returns_unit(&input_fn.sig.output) {
        transform_question_marks(&mut input_fn);
    }

    // Generate the appropriate test attributes
    let is_async = input_fn.sig.asyncness.is_some();
    let test_attrs = if is_async {
        let flavor = attributes
            .flavor
            .unwrap_or(syn::LitStr::new("current_thread", Span::call_site()));

        quote! {
            #[cfg_attr(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")), wasm_bindgen_test::wasm_bindgen_test)]
            #[cfg_attr(not(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none"))), tokio::test(flavor = #flavor))]
        }
    } else {
        quote! {
            #[cfg_attr(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")), wasm_bindgen_test::wasm_bindgen_test)]
            #[cfg_attr(not(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none"))), test)]
        }
    };

    // Combine attributes with the function
    let output = quote! {
        #test_attrs
        #input_fn
    };

    proc_macro::TokenStream::from(output)
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

// Transform ? operators to .unwrap() calls in-place
fn transform_question_marks(input_fn: &mut syn::ItemFn) {
    // Create a visitor that will modify all expressions with ? operators
    struct QuestionMarkVisitor;

    impl syn::visit_mut::VisitMut for QuestionMarkVisitor {
        fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
            // First check if this is a try expression (with ?)
            if let syn::Expr::Try(expr_try) = expr {
                // Get the inner expression that ? is applied to
                let inner = &expr_try.expr;
                // Replace the try expr with an unwrap call
                *expr = syn::parse_quote!( #inner.unwrap() );

                // After replacing, visit the inner expression again
                // in case it also contains ? operators
                self.visit_expr_mut(expr);
                return;
            }

            // If it's not a try expression, visit all child expressions
            syn::visit_mut::visit_expr_mut(self, expr);
        }
    }

    // Apply the visitor to transform all try expressions in the function
    let mut visitor = QuestionMarkVisitor;
    syn::visit_mut::visit_item_fn_mut(&mut visitor, input_fn);
}

#[derive(Default)]
struct Attributes {
    r#async: bool,
    flavor: Option<syn::LitStr>,
}

impl Attributes {
    fn parse(&mut self, meta: syn::meta::ParseNestedMeta) -> syn::parse::Result<()> {
        if meta.path.is_ident("async") {
            self.r#async = true;
        } else if meta.path.is_ident("flavor") {
            self.flavor = Some(meta.value()?.parse::<syn::LitStr>()?);
        } else {
            return Err(meta.error("unknown attribute"));
        }
        Ok(())
    }
}
