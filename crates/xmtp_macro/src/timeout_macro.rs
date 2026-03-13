use proc_macro2::TokenStream;
use quote::quote;
use syn::parse_macro_input;

/// Attribute macro that wraps an async test body with a timeout using
/// `xmtp_common::time::timeout`, which works on both WASM and native.
///
/// This is a drop-in replacement for rstest's `#[timeout]` that works on
/// `wasm32-unknown-unknown`. Use `std::time::Duration` expressions as arguments.
///
/// # Example
///
/// ```ignore
/// #[xmtp_common::test]
/// #[xmtp_common::timeout(std::time::Duration::from_secs(60))]
/// async fn test_something() { ... }
/// ```
pub fn timeout(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let duration_expr = TokenStream::from(attr);
    let input_fn = parse_macro_input!(body as syn::ItemFn);
    expand_timeout(duration_expr, input_fn).into()
}

pub(crate) fn expand_timeout(duration_expr: TokenStream, mut input_fn: syn::ItemFn) -> TokenStream {
    if input_fn.sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            input_fn.sig.fn_token,
            "#[xmtp_common::timeout] can only be applied to async functions",
        )
        .to_compile_error();
    }

    let fn_name_str = input_fn.sig.ident.to_string();
    let original_block = &input_fn.block;

    let new_block: syn::Block = syn::parse_quote! {
        {
            xmtp_common::time::timeout(
                #duration_expr,
                async move #original_block
            )
            .await
            .unwrap_or_else(|_| panic!(
                "test timed out after {:?}: {}",
                #duration_expr,
                #fn_name_str
            ))
        }
    };

    *input_fn.block = new_block;

    quote! { #input_fn }
}
