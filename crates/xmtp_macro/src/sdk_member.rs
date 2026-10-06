//! The `#[sdk(...)]` member options of `sdk_export`.
//!
//! Each option becomes a `#[doc = "@xmtp-..."]` marker. UniFFI carries the
//! marker into the library metadata, where the SDK generator reads it, so the
//! generator needs no table of façade names.

use std::collections::HashSet;

use proc_macro2::Span;
use syn::{
    Attribute, Expr, ExprLit, Fields, FnArg, Ident, ItemEnum, Lit, Meta, ReturnType, Signature,
    Token, Type, punctuated::Punctuated, spanned::Spanned,
};

use crate::sdk_export::returns_result;

const IMMUTABLE: &str = "@xmtp-immutable";
pub(crate) const PURE: &str = "@xmtp-pure";
/// Written by hand in a doc comment: the browser worker makes the call
/// itself, so it never crosses the bridge.
const WORKER: &str = "@xmtp-worker";

const MEMBER_OPTIONS: &str = "unknown sdk option; expected immutable or kind = \"name\"";

/// The target an export is limited to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Native,
    Wasm,
}

impl Target {
    pub(crate) fn from_path(path: &syn::Path) -> Option<Self> {
        if path.is_ident("native_only") {
            Some(Self::Native)
        } else if path.is_ident("wasm_only") {
            Some(Self::Wasm)
        } else {
            None
        }
    }

    pub(crate) fn cfg(self) -> Attribute {
        match self {
            Self::Native => syn::parse_quote!(#[cfg(not(target_arch = "wasm32"))]),
            Self::Wasm => syn::parse_quote!(#[cfg(target_arch = "wasm32")]),
        }
    }
}

/// The `#[sdk(...)]` options of one member.
#[derive(Default)]
struct MemberOptions {
    immutable: Option<Span>,
    kind: Option<(Span, String)>,
}

impl MemberOptions {
    fn reject_immutable(&self) -> syn::Result<()> {
        match self.immutable {
            Some(span) => Err(syn::Error::new(
                span,
                "#[sdk(immutable)] applies to methods",
            )),
            None => Ok(()),
        }
    }

    fn reject_kind(&self) -> syn::Result<()> {
        match &self.kind {
            Some((span, _)) => Err(syn::Error::new(
                *span,
                "#[sdk(kind = ...)] applies to enum variants",
            )),
            None => Ok(()),
        }
    }
}

fn kind_value(value: &Expr) -> Option<String> {
    let Expr::Lit(ExprLit {
        lit: Lit::Str(text),
        ..
    }) = value
    else {
        return None;
    };
    let kind = text.value();
    let valid = !kind.is_empty()
        && kind
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
    valid.then_some(kind)
}

/// Remove every `#[sdk(...)]` attribute and return its options.
fn take(attrs: &mut Vec<Attribute>) -> syn::Result<MemberOptions> {
    let mut options = MemberOptions::default();
    let mut kept = Vec::with_capacity(attrs.len());
    for attr in attrs.drain(..) {
        if !attr.path().is_ident("sdk") {
            kept.push(attr);
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            return Err(syn::Error::new_spanned(&attr, MEMBER_OPTIONS));
        };
        for meta in list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)? {
            let span = meta.span();
            let repeated = || syn::Error::new(span, "sdk option is repeated");
            match &meta {
                Meta::Path(path) if path.is_ident("immutable") => {
                    if options.immutable.replace(span).is_some() {
                        return Err(repeated());
                    }
                }
                Meta::NameValue(pair) if pair.path.is_ident("kind") => {
                    let kind = kind_value(&pair.value).ok_or_else(|| {
                        syn::Error::new(
                            span,
                            "kind takes lowercase words joined by `_` and `.`, such as \"conversation.joined\"",
                        )
                    })?;
                    if options.kind.replace((span, kind)).is_some() {
                        return Err(repeated());
                    }
                }
                other => return Err(syn::Error::new_spanned(other, MEMBER_OPTIONS)),
            }
        }
    }
    *attrs = kept;
    Ok(options)
}

pub(crate) fn push_marker(attrs: &mut Vec<Attribute>, marker: &str) {
    attrs.push(syn::parse_quote!(#[doc = #marker]));
}

/// Whether a doc comment of the member carries `marker` as a word.
fn documents(attrs: &[Attribute], marker: &str) -> bool {
    attrs.iter().any(|attr| match &attr.meta {
        Meta::NameValue(pair) if pair.path.is_ident("doc") => match &pair.value {
            Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) => text.value().split_whitespace().any(|word| word == marker),
            _ => false,
        },
        _ => false,
    })
}

/// A synchronous `&self` method without arguments that returns a value. The
/// SDKs expose it as a readonly property, and the browser bridge reads it
/// once, from a snapshot taken when the handle is made.
fn is_sync_getter(signature: &Signature) -> bool {
    let returns_value = match &signature.output {
        ReturnType::Default => false,
        ReturnType::Type(_, ty) => {
            !matches!(ty.as_ref(), Type::Tuple(tuple) if tuple.elems.is_empty())
                && !returns_result(&signature.output)
        }
    };
    signature.asyncness.is_none()
        && signature.inputs.len() == 1
        && matches!(signature.inputs.first(), Some(FnArg::Receiver(_)))
        && returns_value
}

/// Apply the options of one exported method. `target` is the target of its
/// impl block or trait.
pub(crate) fn method(
    attrs: &mut Vec<Attribute>,
    signature: &Signature,
    target: Option<Target>,
) -> syn::Result<()> {
    let options = take(attrs)?;
    options.reject_kind()?;
    // The browser bridge snapshots the getters it forwards. It forwards
    // neither a worker-only call nor a native-only item. The native-only
    // exemption goes with the conformance constructor probe, which reads live
    // state through native-only getters.
    let bridged = target != Some(Target::Native) && !documents(attrs, WORKER);
    match (options.immutable, is_sync_getter(signature)) {
        (Some(_), true) => push_marker(attrs, IMMUTABLE),
        (Some(span), false) => {
            return Err(syn::Error::new(
                span,
                "#[sdk(immutable)] needs a synchronous `&self` method without arguments that returns a value, not a Result",
            ));
        }
        (None, true) if bridged => {
            return Err(syn::Error::new_spanned(
                &signature.ident,
                format!(
                    "`{}` is a synchronous getter; mark it #[sdk(immutable)] when its value never changes for the object's lifetime, or make it async",
                    signature.ident
                ),
            ));
        }
        (None, _) => {}
    }
    Ok(())
}

/// A free function takes its options in `#[sdk_export(...)]`.
pub(crate) fn function(attrs: &mut Vec<Attribute>, name: &Ident) -> syn::Result<()> {
    let options = take(attrs)?;
    match options.immutable.or(options.kind.map(|(span, _)| span)) {
        Some(span) => Err(syn::Error::new(
            span,
            format!("`{name}` is a free function; it takes its options in #[sdk_export(...)]"),
        )),
        None => Ok(()),
    }
}

/// Apply the options of the fields of a record or of one enum variant.
pub(crate) fn fields(fields: &mut Fields) -> syn::Result<()> {
    for field in fields {
        let options = take(&mut field.attrs)?;
        options.reject_immutable()?;
        options.reject_kind()?;
    }
    Ok(())
}

/// Apply the options of the variants of an enum. `#[sdk(kind = "...")]`
/// names the public string of a variant; an enum marks every variant or none,
/// and each kind once.
pub(crate) fn variants(item: &mut ItemEnum) -> syn::Result<()> {
    let mut kinds = HashSet::new();
    let mut unmarked = None;
    for variant in &mut item.variants {
        let options = take(&mut variant.attrs)?;
        options.reject_immutable()?;
        match options.kind {
            Some((span, kind)) => {
                let marker = format!("@xmtp-kind={kind}");
                if !kinds.insert(kind) {
                    return Err(syn::Error::new(span, "kind is repeated in this enum"));
                }
                push_marker(&mut variant.attrs, &marker);
            }
            None => {
                unmarked.get_or_insert_with(|| variant.ident.clone());
            }
        }
        fields(&mut variant.fields)?;
    }
    match unmarked {
        Some(variant) if !kinds.is_empty() => Err(syn::Error::new_spanned(
            &variant,
            format!(
                "`{}`: every variant needs #[sdk(kind = \"...\")] when one variant has it; `{variant}` has none",
                item.ident
            ),
        )),
        _ => Ok(()),
    }
}
