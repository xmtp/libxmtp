//! The `#[sdk(...)]` member options of `sdk_export`.
//!
//! Each option but `shown` becomes a `#[doc = "@xmtp-..."]` marker. UniFFI
//! carries the marker into the library metadata, where the SDK generator
//! reads it, so the generator needs no table of façade names.

use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Attribute, Expr, ExprLit, Fields, FnArg, Generics, Ident, ImplItem, Item, ItemEnum, Lit, Meta,
    Receiver, ReturnType, Signature, Token, TraitItem, Type, punctuated::Punctuated,
    spanned::Spanned,
};

use crate::sdk_export::returns_result;

/// An asynchronous free function that every SDK also exposes as a static
/// member of its Client.
pub(crate) const CLIENT_STATIC: &str = "@xmtp-client-static";
const IMMUTABLE: &str = "@xmtp-immutable";
const HOST_INTERNAL: &str = "@xmtp-host-internal";
const STREAM: &str = "@xmtp-stream";
const KIND: &str = "@xmtp-kind";
pub(crate) const PURE: &str = "@xmtp-pure";
const REDACT: &str = "@xmtp-redact";
/// On a record or enum with a redacted field: the macro checked its fields
/// and implements its `Debug`. The generator reads `@xmtp-redact` only there.
const REDACTED: &str = "@xmtp-redacted";
/// Written by hand in a doc comment: the browser worker makes the call
/// itself, so it never crosses the bridge.
const WORKER: &str = "@xmtp-worker";

const MEMBER_OPTIONS: &str = "unknown sdk option; expected immutable, host_internal, stream(...), kind = \"name\", redact, redact = \"key\", or shown";

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
    host_internal: Option<Span>,
    stream: Option<(Span, String)>,
    redact: Option<(Span, Redact)>,
    shown: Option<Span>,
    kind: Option<(Span, String)>,
}

impl MemberOptions {
    fn reject_immutable(&self) -> syn::Result<()> {
        if let Some((span, _)) = &self.stream {
            return Err(syn::Error::new(
                *span,
                "#[sdk(stream(...))] applies to object reader methods",
            ));
        }
        if let Some(span) = self.host_internal {
            return Err(syn::Error::new(
                span,
                "#[sdk(host_internal)] applies to object methods",
            ));
        }
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

/// Stream parameters are names, not host-language expressions.
fn stream_value(list: &syn::MetaList) -> syn::Result<String> {
    let mut parameters = std::collections::BTreeMap::new();
    for parameter in list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)? {
        let Meta::NameValue(pair) = &parameter else {
            return Err(syn::Error::new_spanned(
                parameter,
                "stream needs name, options, and owner string parameters",
            ));
        };
        let key = pair
            .path
            .get_ident()
            .map(ToString::to_string)
            .unwrap_or_default();
        if !["name", "options", "owner"].contains(&key.as_str()) {
            return Err(syn::Error::new_spanned(
                parameter,
                "unknown stream parameter; expected name, options, or owner",
            ));
        }
        let value = string_value(&pair.value)
            .filter(|value| {
                value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && syn::parse_str::<Ident>(value).is_ok()
            })
            .ok_or_else(|| {
                syn::Error::new_spanned(
                    &pair.value,
                    "stream parameters need an ASCII identifier string",
                )
            })?;
        if parameters.insert(key, value).is_some() {
            return Err(syn::Error::new_spanned(
                parameter,
                "stream parameter is repeated",
            ));
        }
    }
    let values = ["name", "options", "owner"].map(|key| {
        parameters
            .get(key)
            .cloned()
            .ok_or_else(|| syn::Error::new_spanned(list, format!("stream is missing {key}")))
    });
    let [name, options, owner] = values;
    Ok(format!("{}:{}:{}", name?, options?, owner?))
}

fn type_arguments<'a>(ty: &'a Type, name: &str) -> Option<Vec<&'a Type>> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != name {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    arguments
        .args
        .iter()
        .map(|argument| match argument {
            syn::GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
        .collect()
}

fn stream_signature(signature: &Signature) -> bool {
    let Some(receiver) = signature.receiver() else {
        return false;
    };
    if signature.asyncness.is_none() || !reads_only(receiver) || signature.inputs.len() != 2 {
        return false;
    }
    let Some(FnArg::Typed(input)) = signature.inputs.last() else {
        return false;
    };
    let Some(input) = type_arguments(&input.ty, "Option") else {
        return false;
    };
    if input.len() != 1 || !matches!(input[0], Type::Path(_)) {
        return false;
    }
    let ReturnType::Type(_, output) = &signature.output else {
        return false;
    };
    let Some(output) = type_arguments(output, "Result") else {
        return false;
    };
    if output.len() != 2 {
        return false;
    }
    let Type::Path(error) = output[1] else {
        return false;
    };
    if !error
        .path
        .segments
        .last()
        .is_some_and(|part| part.ident == "XmtpError")
    {
        return false;
    }
    let Some(reader) = type_arguments(output[0], "Arc") else {
        return false;
    };
    if reader.len() != 1 {
        return false;
    }
    let Type::Path(reader) = reader[0] else {
        return false;
    };
    reader
        .path
        .segments
        .last()
        .is_some_and(|part| part.ident == "MessageReader" || part.ident == "ConversationReader")
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
                Meta::List(list) if list.path.is_ident("stream") => {
                    if options
                        .stream
                        .replace((span, stream_value(list)?))
                        .is_some()
                    {
                        return Err(repeated());
                    }
                }
                Meta::Path(path) if path.is_ident("host_internal") => {
                    if options.host_internal.replace(span).is_some() {
                        return Err(repeated());
                    }
                }
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

/// The paths in the `#[derive(...)]` attributes that the macro sees.
fn derives(attrs: &[Attribute]) -> impl Iterator<Item = syn::Path> + '_ {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("derive"))
        .filter_map(|attr| {
            attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
                .ok()
        })
        .flatten()
}

/// Whether a record or enum derives `uniffi::Error`. UniFFI's Kotlin binding
/// renames such a type to an exception class, which the generated
/// diagnostics do not reach.
pub(crate) fn derives_uniffi_error(attrs: &[Attribute]) -> bool {
    derives(attrs).any(|path| {
        let names = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string());
        names.eq(["uniffi", "Error"])
    })
}

/// The option that writes each marker. The generator trusts these markers,
/// so the macro rejects one written in a doc comment, where it would skip
/// the macro's checks.
pub(crate) const WRITTEN_BY_OPTIONS: &[(&str, &str)] = &[
    (CLIENT_STATIC, "#[sdk_export(client_static)]"),
    (HOST_INTERNAL, "#[sdk(host_internal)]"),
    (STREAM, "#[sdk(stream(...))]"),
    (IMMUTABLE, "#[sdk(immutable)]"),
    (KIND, "#[sdk(kind = \"...\")]"),
    (PURE, "#[sdk_export(pure)]"),
    (REDACT, "#[sdk(redact)]"),
    (REDACTED, "#[sdk(redact)] on a field"),
];

/// What a doc attribute writes that the macro rejects.
enum Written {
    /// A marker the generator trusts, and the option that writes it.
    Marker(&'static str, &'static str),
    /// A value that is not a string literal, such as `concat!(...)`. The
    /// macro cannot read it, and it may expand into a marker.
    Computed,
}

/// What a doc attribute writes that the macro rejects, also inside
/// `cfg_attr`, which expands before UniFFI reads the docstring.
fn written(meta: &Meta) -> Option<Written> {
    match meta {
        Meta::NameValue(pair) if pair.path.is_ident("doc") => {
            let Some(text) = string_value(&pair.value) else {
                return Some(Written::Computed);
            };
            text.split_whitespace().find_map(|word| {
                let name = word.split_once('=').map_or(word, |(name, _)| name);
                WRITTEN_BY_OPTIONS
                    .iter()
                    .find(|(marker, _)| *marker == name)
                    .map(|&(marker, option)| Written::Marker(marker, option))
            })
        }
        Meta::List(list) if list.path.is_ident("cfg_attr") => list
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .ok()?
            .iter()
            .skip(1)
            .find_map(written),
        _ => None,
    }
}

fn reject_written(attrs: &[Attribute]) -> syn::Result<()> {
    let Some((attr, written)) = attrs
        .iter()
        .find_map(|attr| Some((attr, written(&attr.meta)?)))
    else {
        return Ok(());
    };
    let message = match written {
        Written::Marker(marker, option) => {
            format!("write {option} instead of `{marker}` in a doc comment")
        }
        Written::Computed => {
            "write the doc comment as a string literal, which the macro checks for markers"
                .to_owned()
        }
    };
    Err(syn::Error::new_spanned(attr, message))
}

/// Reject a marker that a doc comment of the item or of one of its members
/// spells out. Runs before the macro adds its own markers.
pub(crate) fn reject_written_markers(item: &Item) -> syn::Result<()> {
    match item {
        Item::Impl(item_impl) => {
            reject_written(&item_impl.attrs)?;
            for impl_item in &item_impl.items {
                if let ImplItem::Fn(function) = impl_item {
                    reject_written(&function.attrs)?;
                }
            }
        }
        Item::Trait(item_trait) => {
            reject_written(&item_trait.attrs)?;
            for trait_item in &item_trait.items {
                if let TraitItem::Fn(function) = trait_item {
                    reject_written(&function.attrs)?;
                }
            }
        }
        Item::Fn(function) => reject_written(&function.attrs)?,
        Item::Struct(item_struct) => {
            reject_written(&item_struct.attrs)?;
            for field in &item_struct.fields {
                reject_written(&field.attrs)?;
            }
        }
        Item::Enum(item_enum) => {
            reject_written(&item_enum.attrs)?;
            for variant in &item_enum.variants {
                reject_written(&variant.attrs)?;
                for field in &variant.fields {
                    reject_written(&field.attrs)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// The `Debug` of a record or enum with a redacted field: it calls the
/// type's own `fn redacted_debug(&self, f: &mut Formatter<'_>) -> fmt::Result`.
/// Any other `Debug` conflicts with it, so a derived one fails to compile
/// whatever its spelling, its `cfg_attr`, or its place among the attributes.
/// A derive the macro sees gets a clearer error first. The impl takes the
/// `#[cfg]` attributes written on the type.
pub(crate) fn redacted_debug(
    attrs: &mut Vec<Attribute>,
    ident: &Ident,
    generics: &Generics,
) -> syn::Result<TokenStream> {
    if let Some(path) = derives(attrs).find(|path| {
        path.segments
            .last()
            .is_some_and(|segment| segment.ident == "Debug")
    }) {
        return Err(syn::Error::new_spanned(
            path,
            format!(
                "`{ident}` has a #[sdk(redact)] field, so sdk_export implements its Debug through `fn redacted_debug`; remove this derive"
            ),
        ));
    }
    let cfgs = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg"))
        .cloned()
        .collect::<Vec<_>>();
    push_marker(attrs, REDACTED);
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    // The method takes the type's span, so a missing one points at the type.
    let method = Ident::new("redacted_debug", ident.span());
    Ok(quote! {
        #(#cfgs)*
        impl #impl_generics ::core::fmt::Debug for #ident #type_generics #where_clause {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                Self::#method(self, formatter)
            }
        }
    })
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
    object: bool,
) -> syn::Result<()> {
    let options = take(attrs)?;
    options.reject_display()?;
    options.reject_kind()?;
    if let Some((span, value)) = &options.stream {
        if !object || options.host_internal.is_some() || !stream_signature(signature) {
            return Err(syn::Error::new(
                *span,
                "#[sdk(stream(...))] needs an async object reader method with one optional record input and Result<Arc<MessageReader or ConversationReader>, XmtpError>",
            ));
        }
        push_marker(attrs, &format!("{STREAM}={value}"));
    }
    if let Some(span) = options.host_internal {
        if !object || signature.receiver().is_none() {
            return Err(syn::Error::new(
                span,
                "#[sdk(host_internal)] applies to object methods",
            ));
        }
        push_marker(attrs, HOST_INTERNAL);
        push_marker(attrs, "@xmtp-internal");
    }
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
        .or(options.host_internal)
        .or(options.stream.map(|(span, _)| span))
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

/// The fields of a record, or of every variant of an enum, sorted by their
/// `redact` and `shown` options.
#[derive(Default)]
struct Classified {
    redacted: bool,
    shown: Option<Span>,
    /// Named fields with neither option, as `Owner.field`.
    unmarked: Vec<(String, Ident)>,
    /// A tuple variant field, which no option can mark.
    unnamed: Option<Span>,
}

/// Apply the options of `fields`, writing a marker on each redacted one.
/// `owner` names the record or variant in errors.
fn classify(fields: &mut Fields, owner: &str, classified: &mut Classified) -> syn::Result<()> {
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
            classified.unnamed.get_or_insert(field.span());
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
                classified.redacted = true;
            }
            (None, Some(span)) => {
                classified.shown.get_or_insert(span);
            }
            (None, None) => classified.unmarked.push((owner.to_owned(), name)),
        }
    }
    Ok(())
}

/// Say whether a type redacts a field. Redaction fails closed: once one
/// field of a record, or of any variant of an enum, is `#[sdk(redact)]`,
/// every field of the type says `redact` or `shown`, so a new field or
/// variant never prints a secret by default. `item` is the record or enum.
fn redacts(classified: Classified, item: &Ident, uniffi_error: bool) -> syn::Result<bool> {
    if !classified.redacted {
        return match classified.shown {
            Some(span) => Err(syn::Error::new(
                span,
                "#[sdk(shown)] applies in a type with a #[sdk(redact)] field",
            )),
            None => Ok(false),
        };
    }
    if uniffi_error {
        return Err(syn::Error::new_spanned(
            item,
            format!(
                "`{item}` derives uniffi::Error, which the Kotlin binding renames to an exception class; #[sdk(redact)] applies to records and plain enums"
            ),
        ));
    }
    if let Some(span) = classified.unnamed {
        return Err(syn::Error::new(
            span,
            format!(
                "`{item}` has a redacted field, so each of its fields needs a name to take #[sdk(redact)] or #[sdk(shown)]"
            ),
        ));
    }
    match classified.unmarked.first() {
        Some((owner, field)) => Err(syn::Error::new_spanned(
            field,
            format!(
                "`{owner}.{field}` is in a type with a redacted field; mark it #[sdk(redact)] or #[sdk(shown)]"
            ),
        )),
        None => Ok(true),
    }
}

/// Apply the options of the fields of a record, and say whether one of them
/// is redacted. `item` is the record.
pub(crate) fn fields(fields: &mut Fields, item: &Ident, uniffi_error: bool) -> syn::Result<bool> {
    let mut classified = Classified::default();
    classify(fields, &item.to_string(), &mut classified)?;
    redacts(classified, item, uniffi_error)
}

/// Apply the options of the variants of an enum, and say whether a variant
/// field is redacted. A redacted field in one variant makes every field of
/// every variant say `redact` or `shown`. `#[sdk(kind = "...")]` names the
/// public string of a variant; an enum marks every variant or none, and each
/// kind once.
pub(crate) fn variants(item: &mut ItemEnum) -> syn::Result<bool> {
    let uniffi_error = derives_uniffi_error(&item.attrs);
    let mut classified = Classified::default();
    let mut kinds = HashSet::new();
    let mut unmarked = None;
    for variant in &mut item.variants {
        let options = take(&mut variant.attrs)?;
        options.reject_immutable()?;
        options.reject_display()?;
        match options.kind {
            Some((span, kind)) => {
                let marker = format!("{KIND}={kind}");
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
        classify(&mut variant.fields, &owner, &mut classified)?;
    }
    let redacted = redacts(classified, &item.ident, uniffi_error)?;
    match unmarked {
        Some(variant) if !kinds.is_empty() => Err(syn::Error::new_spanned(
            &variant,
            format!(
                "`{}`: every variant needs #[sdk(kind = \"...\")] when one variant has it; `{variant}` has none",
                item.ident
            ),
        )),
        _ => Ok(redacted),
    }
}
