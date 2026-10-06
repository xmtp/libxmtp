//! The `#[sdk(...)]` member options of `sdk_export`.
//!
//! Each option but `shown` becomes a `#[doc = "@xmtp-..."]` marker. UniFFI
//! carries the marker into the library metadata, where the SDK generator
//! reads it, so the generator needs no table of façade names.

use std::collections::HashSet;

use proc_macro2::Span;
use syn::{
    Attribute, Expr, ExprLit, Fields, FnArg, Ident, ItemEnum, Lit, Meta, Receiver, ReturnType,
    Signature, Token, Type, punctuated::Punctuated, spanned::Spanned,
};

use crate::sdk_export::returns_result;

const IMMUTABLE: &str = "@xmtp-immutable";
pub(crate) const PURE: &str = "@xmtp-pure";
const REDACT: &str = "@xmtp-redact";
/// Written by hand in a doc comment: the browser worker makes the call
/// itself, so it never crosses the bridge.
const WORKER: &str = "@xmtp-worker";

const MEMBER_OPTIONS: &str =
    "unknown sdk option; expected immutable, kind = \"name\", redact, redact = \"key\", or shown";

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

/// What `#[sdk(redact)]` hides in generated diagnostic text.
enum Redact {
    /// The whole field.
    Whole,
    /// One key of a string map field.
    Key(String),
}

/// The `#[sdk(...)]` options of one member.
#[derive(Default)]
struct MemberOptions {
    immutable: Option<Span>,
    redact: Option<(Span, Redact)>,
    shown: Option<Span>,
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

    /// `redact` and `shown` describe the fields of a record or variant.
    fn reject_display(&self) -> syn::Result<()> {
        match self.redact.as_ref().map(|(span, _)| *span).or(self.shown) {
            Some(span) => Err(syn::Error::new(
                span,
                "#[sdk(redact)] and #[sdk(shown)] apply to record and variant fields",
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

fn string_value(value: &Expr) -> Option<String> {
    match value {
        Expr::Lit(ExprLit {
            lit: Lit::Str(text),
            ..
        }) => Some(text.value()),
        _ => None,
    }
}

/// A map key that the generator can write into a Kotlin or Swift string
/// literal as it is.
fn redact_key(value: &Expr) -> Option<String> {
    string_value(value).filter(|key| {
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    })
}

fn kind_value(value: &Expr) -> Option<String> {
    let kind = string_value(value)?;
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
                Meta::Path(path) if path.is_ident("redact") => {
                    if options.redact.replace((span, Redact::Whole)).is_some() {
                        return Err(repeated());
                    }
                }
                Meta::NameValue(pair) if pair.path.is_ident("redact") => {
                    let key = redact_key(&pair.value).ok_or_else(|| {
                        syn::Error::new(
                            span,
                            "redact takes one map key of ASCII letters, digits, `_`, `.`, and `-`, such as \"secret\"",
                        )
                    })?;
                    if options.redact.replace((span, Redact::Key(key))).is_some() {
                        return Err(repeated());
                    }
                }
                Meta::Path(path) if path.is_ident("shown") => {
                    if options.shown.replace(span).is_some() {
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

/// The derives of a record or enum that redaction cannot work with. The
/// macro sees only the derives below it, so `sdk_export` comes first.
#[derive(Clone, Copy, Default)]
pub(crate) struct Derives {
    /// A derived `Debug`, under any path, prints every field.
    debug: bool,
    /// UniFFI's Kotlin binding renames a `uniffi::Error` type to an
    /// exception class, which the generated diagnostics do not reach.
    uniffi_error: bool,
}

impl Derives {
    pub(crate) fn of(attrs: &[Attribute]) -> Self {
        let mut derives = Self::default();
        let paths = attrs
            .iter()
            .filter(|attr| attr.path().is_ident("derive"))
            .filter_map(|attr| {
                attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
                    .ok()
            })
            .flatten();
        for path in paths {
            let names = path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>();
            derives.debug |= names.last().is_some_and(|name| name == "Debug");
            derives.uniffi_error |= names == ["uniffi", "Error"];
        }
        derives
    }
}

/// A synchronous `&self` method without arguments that returns a value. The
/// SDKs expose it as a readonly property, and the browser bridge reads it
/// once, from a snapshot taken when the handle is made. `self: Arc<Self>`
/// reads the same way; `&mut self` and `self` make no getter.
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
        && matches!(signature.inputs.first(), Some(FnArg::Receiver(receiver)) if reads_only(receiver))
        && returns_value
}

/// A shared reference or an `Arc`: UniFFI takes `&self` and `self: Arc<Self>`.
fn reads_only(receiver: &Receiver) -> bool {
    match receiver.ty.as_ref() {
        Type::Reference(reference) => reference.mutability.is_none(),
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Arc"),
        _ => false,
    }
}

/// Apply the options of one exported method. `target` is the target of its
/// impl block or trait.
pub(crate) fn method(
    attrs: &mut Vec<Attribute>,
    signature: &Signature,
    target: Option<Target>,
) -> syn::Result<()> {
    let options = take(attrs)?;
    options.reject_display()?;
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
    let span = options
        .immutable
        .or(options.redact.map(|(span, _)| span))
        .or(options.shown)
        .or(options.kind.map(|(span, _)| span));
    match span {
        Some(span) => Err(syn::Error::new(
            span,
            format!("`{name}` is a free function; it takes its options in #[sdk_export(...)]"),
        )),
        None => Ok(()),
    }
}

/// Apply the options of the fields of a record or of one enum variant.
///
/// Redaction fails closed: once one field is `#[sdk(redact)]`, every other
/// field says `redact` or `shown`, so a new field never prints a secret by
/// default. `item` is the record or enum, and `derives` holds the derives
/// that a redacted field rules out.
pub(crate) fn fields(
    fields: &mut Fields,
    item: &Ident,
    owner: &str,
    derives: Derives,
) -> syn::Result<()> {
    let mut redacted = None;
    let mut shown = None;
    let mut unmarked = Vec::new();
    for field in fields.iter_mut() {
        let options = take(&mut field.attrs)?;
        options.reject_immutable()?;
        options.reject_kind()?;
        let Some(name) = field.ident.clone() else {
            options.reject_display().map_err(|error| {
                syn::Error::new(
                    error.span(),
                    "#[sdk(redact)] and #[sdk(shown)] need a named field",
                )
            })?;
            continue;
        };
        match (options.redact, options.shown) {
            (Some((span, _)), Some(_)) => {
                return Err(syn::Error::new(
                    span,
                    "a field is either #[sdk(redact)] or #[sdk(shown)]",
                ));
            }
            (Some((_, redact)), None) => {
                let marker = match redact {
                    Redact::Whole => REDACT.to_owned(),
                    Redact::Key(key) => format!("{REDACT}={key}"),
                };
                push_marker(&mut field.attrs, &marker);
                redacted.get_or_insert(name);
            }
            (None, Some(span)) => {
                shown.get_or_insert(span);
            }
            (None, None) => unmarked.push(name),
        }
    }
    let Some(redacted) = redacted else {
        return match shown {
            Some(span) => Err(syn::Error::new(
                span,
                "#[sdk(shown)] applies beside a #[sdk(redact)] field",
            )),
            None => Ok(()),
        };
    };
    if derives.uniffi_error {
        return Err(syn::Error::new_spanned(
            item,
            format!(
                "`{item}` derives uniffi::Error, which the Kotlin binding renames to an exception class; #[sdk(redact)] applies to records and plain enums"
            ),
        ));
    }
    if derives.debug {
        return Err(syn::Error::new_spanned(
            item,
            format!(
                "`{item}` derives Debug, which prints `{redacted}`; write an `impl Debug` that redacts it. Keep sdk_export the first attribute, above every derive: it cannot see a derive written above it"
            ),
        ));
    }
    match unmarked.first() {
        Some(field) => Err(syn::Error::new_spanned(
            field,
            format!(
                "`{owner}.{field}` sits beside a redacted field; mark it #[sdk(redact)] or #[sdk(shown)]"
            ),
        )),
        None => Ok(()),
    }
}

/// Apply the options of the variants of an enum. `#[sdk(kind = "...")]`
/// names the public string of a variant; an enum marks every variant or none,
/// and each kind once.
pub(crate) fn variants(item: &mut ItemEnum) -> syn::Result<()> {
    let derives = Derives::of(&item.attrs);
    let mut kinds = HashSet::new();
    let mut unmarked = None;
    for variant in &mut item.variants {
        let options = take(&mut variant.attrs)?;
        options.reject_immutable()?;
        options.reject_display()?;
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
        let owner = format!("{}::{}", item.ident, variant.ident);
        fields(&mut variant.fields, &item.ident, &owner, derives)?;
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
