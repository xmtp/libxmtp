use crate::timeout_macro::expand_timeout;
use quote::quote;

#[test]
fn test_async_function_wraps_with_timeout() {
    let duration = quote! { std::time::Duration::from_secs(60) };
    let input_fn: syn::ItemFn = syn::parse_quote! {
        async fn test_something() {
            assert!(true);
        }
    };

    let output = expand_timeout(duration, input_fn);
    let output_str = output.to_string();

    assert!(output_str.contains("xmtp_common :: time :: timeout"));
    assert!(output_str.contains("test_something"));
    assert!(output_str.contains("unwrap_or_else"));
    assert!(output_str.contains("timed out after"));
}

#[test]
fn test_timeout_embeds_function_name_in_panic_message() {
    let duration = quote! { std::time::Duration::from_secs(30) };
    let input_fn: syn::ItemFn = syn::parse_quote! {
        async fn my_specific_test() {}
    };

    let output = expand_timeout(duration, input_fn);
    let output_str = output.to_string();

    assert!(output_str.contains("my_specific_test"));
}

#[test]
fn test_non_async_function_returns_compile_error() {
    let duration = quote! { std::time::Duration::from_secs(60) };
    let input_fn: syn::ItemFn = syn::parse_quote! {
        fn test_something() {}
    };

    let output = expand_timeout(duration, input_fn);
    let output_str = output.to_string();

    assert!(output_str.contains("compile_error"));
}
