use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Meta, Token, parse::Parse, parse::ParseStream, parse_macro_input, punctuated::Punctuated,
};

/// Expand a span attribute into libxmtp's one canonical, OTEL-safe instrument
/// form, deriving the operation as `"<prefix>.<fn_name>"`:
///
/// ```ignore
/// #[tracing::instrument(err, skip_all, fields(operation = "<prefix>.<fn_name>"))]
/// ```
///
/// `err` records span status=error on an `Err` return (feeding
/// `<namespace>.calls{status.code}` in the Collector). `skip_all` keeps every
/// argument OUT of the span, so a per-call id (group_id / inbox_id / cursor) can
/// never leak in and explode trace-attribute cardinality on the OTEL export.
/// `operation` is the single dimension the Collector's `span_metrics` connector
/// buckets on. Making this the only writable form guarantees those invariants at
/// compile time — no runtime test required.
pub(crate) fn expand_with_prefix(prefix: &str, input_fn: syn::ItemFn) -> TokenStream {
    let operation = format!("{}.{}", prefix, input_fn.sig.ident);
    quote! {
        #[tracing::instrument(err, skip_all, fields(operation = #operation))]
        #input_fn
    }
}

/// `#[rpc_span]` → `operation = "rpc.<fn_name>"`. For `ApiClientWrapper` RPC
/// methods; surfaces as `xmtp.api.*` Collector metrics.
pub fn rpc_span(
    _attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input_fn = parse_macro_input!(body as syn::ItemFn);
    expand_with_prefix("rpc", input_fn).into()
}

/// `#[db_span]` → `operation = "db.<fn_name>"`. For `xmtp_db` query methods;
/// surfaces as `xmtp.db.*` Collector metrics.
pub fn db_span(
    _attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input_fn = parse_macro_input!(body as syn::ItemFn);
    expand_with_prefix("db", input_fn).into()
}

/// `#[mls_span]` → `operation = "mls.<fn_name>"`. For high-level MLS operations
/// (sync, intent, send); surfaces as `xmtp.mls.*` Collector metrics.
pub fn mls_span(
    _attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input_fn = parse_macro_input!(body as syn::ItemFn);
    expand_with_prefix("mls", input_fn).into()
}

/// `#[span(prefix = "...")]` → `operation = "<prefix>.<fn_name>"`. Escape hatch
/// for a namespace without a dedicated attribute; prefer `#[rpc_span]` /
/// `#[db_span]` / `#[mls_span]` where they apply.
pub fn span(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let args = parse_macro_input!(attr as SpanArgs);
    let input_fn = parse_macro_input!(body as syn::ItemFn);
    expand_with_prefix(&args.prefix, input_fn).into()
}

/// Parses the `prefix = "..."` argument of `#[span(...)]`.
struct SpanArgs {
    prefix: String,
}

impl Parse for SpanArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;
        for meta in &metas {
            if let Meta::NameValue(nv) = meta
                && nv.path.is_ident("prefix")
                && let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
            {
                return Ok(SpanArgs { prefix: s.value() });
            }
        }
        Err(syn::Error::new(
            input.span(),
            r#"#[span] requires a string prefix: #[span(prefix = "rpc")]"#,
        ))
    }
}
