extern crate proc_macro;

use proc_macro2::*;
use quote::quote;

/// A test macro that delegates to the appropriate test framework based on the target architecture.
///
/// On wasm32 architecture, it delegates to `wasm_bindgen_test::wasm_bindgen_test`.
/// On all other architectures, it delegates to `tokio::test`.
///
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
    let mut tokens = Vec::<TokenTree>::new();

    syn::parse_macro_input!(attr with attribute_parser);

    let mut body = TokenStream::from(body).into_iter().peekable();

    // Skip over other attributes to `fn #ident ...`, and extract `#ident`
    let mut leading_tokens = Vec::new();
    while let Some(token) = body.next() {
        leading_tokens.push(token.clone());
        if let TokenTree::Ident(token) = token {
            if token == "async" {
                attributes.r#async = true;
            }
            if token == "fn" {
                break;
            }
        }
    }

    let ident = find_ident(&mut body).expect("expected a function name");

    tokens.extend( quote! {
        #[cfg_attr(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")), wasm_bindgen_test::wasm_bindgen_test)]
    });

    if attributes.flavor.is_some() && attributes.r#async {
        let flavor = attributes.flavor.expect("checked for none");
        tokens.extend(quote!{
            #[cfg_attr(not(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none"))), tokio::test(flavor = #flavor))]
        });
    } else if attributes.r#async {
        tokens.extend(quote!{
            #[cfg_attr(not(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none"))), tokio::test(flavor = "current_thread"))]
        });
    } else {
        tokens.extend(quote!{
            #[cfg_attr(not(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none"))), test)]
        });
    }

    tokens.extend(leading_tokens);
    tokens.push(ident.into());
    tokens.extend(body);
    // Return the modified token stream
    tokens.into_iter().collect::<TokenStream>().into()
}

fn find_ident(iter: &mut impl Iterator<Item = TokenTree>) -> Option<Ident> {
    match iter.next()? {
        TokenTree::Ident(i) => Some(i),
        TokenTree::Group(g) if g.delimiter() == Delimiter::None => {
            find_ident(&mut g.stream().into_iter())
        }
        _ => None,
    }
}

struct Attributes {
    r#async: bool,
    flavor: Option<syn::LitStr>,
}

impl Default for Attributes {
    fn default() -> Self {
        Self {
            r#async: false,
            flavor: None,
        }
    }
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
