use std::collections::HashSet;

use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::{ToTokens, format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Expr, Fields, parse_macro_input};

/// How a single variant (or struct) decides its retryability.
enum Decision {
    /// `#[retry(true)]` / bare `#[retry]` → always `true`.
    AlwaysTrue,
    /// `#[retry(false)]` → always `false`.
    AlwaysFalse,
    /// `#[retry(inherit)]` → forward to the inner field's `is_retryable()`.
    Forward,
    /// `#[retry(when = EXPR)]` → run `EXPR` with the variant's fields in scope.
    /// Boxed because `syn::Expr` is large relative to the other (unit) variants.
    When(Box<Expr>),
    /// No `#[retry(...)]` and no `#[from]` → fall back to the container baseline.
    Default,
}

/// Parsed `#[retry(...)]` attribute(s) on a variant or container.
#[derive(Default)]
struct RetryAttr {
    always_true: bool,
    always_false: bool,
    inherit: bool,
    when: Option<Expr>,
    /// Container-only: `#[retry(default = true|false)]`.
    default: Option<bool>,
    /// Span of the attribute, for error reporting.
    span: Option<Span>,
}

impl RetryAttr {
    fn parse(attrs: &[syn::Attribute]) -> syn::Result<Self> {
        let mut result = Self::default();

        // Reject a key that is already set: without this, a duplicated or
        // conflicting attribute (`#[retry(default = true)] #[retry(default =
        // false)]`) would silently let the last one win and flip
        // `is_retryable()` results with no diagnostic.
        fn set(flag: &mut bool, span: Span, key: &str) -> syn::Result<()> {
            if *flag {
                return Err(syn::Error::new(
                    span,
                    format!("duplicate `#[retry(...)]` key `{key}`"),
                ));
            }
            *flag = true;
            Ok(())
        }

        for attr in attrs {
            if !attr.path().is_ident("retry") {
                continue;
            }
            result.span = Some(attr.span());

            // Bare `#[retry]` with no arguments → always true.
            if matches!(attr.meta, syn::Meta::Path(_)) {
                set(&mut result.always_true, attr.span(), "true")?;
                continue;
            }

            // `#[retry()]` would otherwise sail through `parse_nested_meta`
            // without invoking the callback and silently take the baseline.
            if matches!(&attr.meta, syn::Meta::List(l) if l.tokens.is_empty()) {
                return Err(syn::Error::new(
                    attr.span(),
                    "empty `#[retry()]`: use bare `#[retry]` for `true`, or specify a key \
                     (`true`, `false`, `inherit`, `when = <expr>`, `default = <bool>`)",
                ));
            }

            attr.parse_nested_meta(|meta| {
                let span = meta.path.span();
                if meta.path.is_ident("true") {
                    set(&mut result.always_true, span, "true")
                } else if meta.path.is_ident("false") {
                    set(&mut result.always_false, span, "false")
                } else if meta.path.is_ident("inherit") {
                    set(&mut result.inherit, span, "inherit")
                } else if meta.path.is_ident("when") {
                    if result.when.is_some() {
                        return Err(syn::Error::new(span, "duplicate `#[retry(...)]` key `when`"));
                    }
                    let value = meta.value()?;
                    result.when = Some(value.parse()?);
                    Ok(())
                } else if meta.path.is_ident("default") {
                    if result.default.is_some() {
                        return Err(syn::Error::new(
                            span,
                            "duplicate `#[retry(...)]` key `default`",
                        ));
                    }
                    let value = meta.value()?;
                    let lit: syn::LitBool = value.parse()?;
                    result.default = Some(lit.value);
                    Ok(())
                } else {
                    Err(meta.error(
                        "expected `true`, `false`, `inherit`, `when = <expr>`, or `default = <bool>`",
                    ))
                }
            })?;
        }

        Ok(result)
    }

    fn span(&self) -> Span {
        self.span.unwrap_or_else(Span::call_site)
    }

    /// Validate this as the container attribute of an enum: only `default` is
    /// meaningful there — the other keys act on variants.
    fn validate_enum_container(&self) -> syn::Result<()> {
        if self.always_true || self.always_false || self.inherit || self.when.is_some() {
            return Err(syn::Error::new(
                self.span(),
                "only `#[retry(default = <bool>)]` is valid on an enum; \
                 set per-variant behavior with `#[retry(...)]` on the variants",
            ));
        }
        Ok(())
    }

    /// Validate this as the container attribute of a struct: `inherit` has no
    /// meaning (there is no variant field selection), and the remaining keys
    /// are mutually exclusive.
    fn validate_struct_container(&self) -> syn::Result<()> {
        if self.inherit {
            return Err(syn::Error::new(
                self.span(),
                "`#[retry(inherit)]` is not valid on a struct; \
                 use `#[retry(when = self.<field>.is_retryable())]` to forward",
            ));
        }
        let set = [
            self.always_true,
            self.always_false,
            self.when.is_some(),
            self.default.is_some(),
        ]
        .into_iter()
        .filter(|b| *b)
        .count();
        if set > 1 {
            return Err(syn::Error::new(
                self.span(),
                "conflicting `#[retry(...)]` keys on a struct: use exactly one of \
                 `true`, `false`, `when`, or `default`",
            ));
        }
        Ok(())
    }

    /// Resolve the decision for a variant.
    ///
    /// `#[from]` deliberately carries no retry semantics: a workspace census
    /// found 98 `#[from]` variants needing a hardcoded override vs 33 that
    /// forward — auto-forwarding was the minority case and forced noisy
    /// `#[retry(false)]` annotations on every foreign-wrapping variant.
    /// Forwarding is always explicit via `#[retry(inherit)]`.
    fn decision(self, fallback_span: Span) -> syn::Result<Decision> {
        let span = self.span.unwrap_or(fallback_span);

        if self.default.is_some() {
            return Err(syn::Error::new(
                span,
                "`#[retry(default = ...)]` is only valid on the enum or struct itself, not on a variant",
            ));
        }

        // Count how many mutually-exclusive decision keys were set.
        let set = [
            self.always_true,
            self.always_false,
            self.inherit,
            self.when.is_some(),
        ]
        .into_iter()
        .filter(|b| *b)
        .count();
        if set > 1 {
            return Err(syn::Error::new(
                span,
                "conflicting `#[retry(...)]` keys: use exactly one of `true`, `false`, `inherit`, or `when`",
            ));
        }

        Ok(if let Some(expr) = self.when {
            Decision::When(Box::new(expr))
        } else if self.always_true {
            Decision::AlwaysTrue
        } else if self.always_false {
            Decision::AlwaysFalse
        } else if self.inherit {
            Decision::Forward
        } else {
            Decision::Default
        })
    }
}

pub fn derive_retryable(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let container = match RetryAttr::parse(&input.attrs) {
        Ok(c) => c,
        Err(e) => return e.to_compile_error().into(),
    };

    let body = match &input.data {
        Data::Enum(data_enum) => {
            if let Err(e) = container.validate_enum_container() {
                return e.to_compile_error().into();
            }
            let baseline = container.default.unwrap_or(false);

            if data_enum.variants.is_empty() {
                // `match self {}` is rejected for an uninhabited enum — a
                // *reference* to it is still considered inhabited for
                // exhaustiveness. Matching the dereferenced place is allowed.
                quote! { match *self {} }
            } else {
                let arms: Result<Vec<_>, syn::Error> = data_enum
                    .variants
                    .iter()
                    .map(|variant| variant_arm(variant, baseline))
                    .collect();
                match arms {
                    Ok(arms) => quote! {
                        match self {
                            #(#arms)*
                        }
                    },
                    Err(e) => return e.to_compile_error().into(),
                }
            }
        }
        Data::Struct(_) => {
            if let Err(e) = container.validate_struct_container() {
                return e.to_compile_error().into();
            }
            struct_body(&container)
        }
        Data::Union(_) => {
            return syn::Error::new_spanned(&input, "`Retryable` cannot be derived for unions")
                .to_compile_error()
                .into();
        }
    };

    quote! {
        #[automatically_derived]
        impl #impl_generics xmtp_common::RetryableError for #name #ty_generics #where_clause {
            fn is_retryable(&self) -> bool {
                #body
            }
        }
    }
    .into()
}

/// Build the `is_retryable` body for a struct: either a `when` expression over
/// `self`, or a constant (`true`/`false`/bare `#[retry]`, else the `default`
/// baseline). Container validation has already rejected conflicting keys.
fn struct_body(container: &RetryAttr) -> TokenStream2 {
    if let Some(expr) = &container.when {
        return quote! { #expr };
    }
    let value = container.always_true || container.default.unwrap_or(false);
    quote! { #value }
}

/// Build a single `match` arm for one enum variant.
fn variant_arm(variant: &syn::Variant, baseline: bool) -> syn::Result<TokenStream2> {
    let vname = &variant.ident;
    let attr = RetryAttr::parse(&variant.attrs)?;
    let decision = attr.decision(variant.span())?;

    let arm = match decision {
        Decision::AlwaysTrue => quote! { Self::#vname { .. } => true, },
        Decision::AlwaysFalse => quote! { Self::#vname { .. } => false, },
        Decision::Default => {
            quote! { Self::#vname { .. } => #baseline, }
        }
        Decision::Forward => forward_arm(vname, &variant.fields)?,
        Decision::When(expr) => when_arm(vname, &variant.fields, *expr),
    };
    Ok(arm)
}

/// `Self::V(inner) => inner.is_retryable()` — forward to the single inner field.
fn forward_arm(vname: &syn::Ident, fields: &Fields) -> syn::Result<TokenStream2> {
    match fields {
        Fields::Unnamed(f) if f.unnamed.len() == 1 => Ok(quote! {
            Self::#vname(inner) => inner.is_retryable(),
        }),
        Fields::Named(f) if f.named.len() == 1 => {
            let fname = f.named.first().unwrap().ident.as_ref().unwrap();
            Ok(quote! {
                Self::#vname { #fname } => #fname.is_retryable(),
            })
        }
        _ => Err(syn::Error::new_spanned(
            vname,
            "`#[retry(inherit)]` requires exactly one field",
        )),
    }
}

/// Collect every identifier appearing in a token stream, recursing into groups.
///
/// Used to decide which variant fields a `when` expression references; an
/// over-approximation (e.g. an identifier inside a nested macro call) only
/// costs an extra binding, never a miss.
fn collect_idents(ts: TokenStream2, out: &mut HashSet<String>) {
    for tt in ts {
        match tt {
            TokenTree::Ident(i) => {
                out.insert(i.to_string());
            }
            TokenTree::Group(g) => collect_idents(g.stream(), out),
            _ => {}
        }
    }
}

/// `Self::V(this) => { EXPR }` — bind the referenced fields and run the custom
/// expression. Only fields the expression mentions are bound, so an unused
/// field never produces an `unused_variables` warning under `-D warnings`.
fn when_arm(vname: &syn::Ident, fields: &Fields, expr: Expr) -> TokenStream2 {
    // Narrow the expression's span to the body block only, so pattern
    // diagnostics keep their own spans.
    let span = expr.span();
    let body = quote_spanned! {span=> { #expr } };

    let mut referenced = HashSet::new();
    collect_idents(expr.to_token_stream(), &mut referenced);

    match fields {
        Fields::Unit => quote! { Self::#vname => #body, },
        Fields::Unnamed(f) => {
            let bindings = (0..f.unnamed.len()).map(|i| {
                let name = if f.unnamed.len() == 1 {
                    format_ident!("this")
                } else {
                    format_ident!("this{}", i)
                };
                if referenced.contains(&name.to_string()) {
                    name.to_token_stream()
                } else {
                    quote! { _ }
                }
            });
            quote! { Self::#vname(#(#bindings),*) => #body, }
        }
        Fields::Named(f) => {
            let used: Vec<_> = f
                .named
                .iter()
                .filter_map(|fld| fld.ident.as_ref())
                .filter(|id| referenced.contains(&id.to_string()))
                .collect();
            quote! { Self::#vname { #(#used,)* .. } => #body, }
        }
    }
}
