use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    ImplItem, Item, Meta, ReturnType, Token, TraitItem, Type, parse::Parser, punctuated::Punctuated,
};

use crate::sdk_member::{self, CLIENT_STATIC, PURE, Target, push_marker};

const EXPORT_OPTIONS: &str =
    "sdk_export accepts native_only, wasm_only, pure, client_static, and default(...)";

/// The arguments of `#[sdk_export(...)]`.
#[derive(Default)]
struct ExportOptions {
    target: Option<Target>,
    pure: bool,
    client_static: bool,
    defaults: TokenStream,
}

fn parse_options(attr: TokenStream) -> syn::Result<ExportOptions> {
    let mut options = ExportOptions::default();
    for argument in Punctuated::<Meta, Token![,]>::parse_terminated.parse2(attr)? {
        let repeated = |what: &str| {
            syn::Error::new_spanned(&argument, format!("sdk_export has more than one {what}"))
        };
        match &argument {
            Meta::Path(path) if Target::from_path(path).is_some() => {
                if options.target.is_some() {
                    return Err(repeated("target"));
                }
                options.target = Target::from_path(path);
            }
            Meta::Path(path) if path.is_ident("pure") => {
                if options.pure {
                    return Err(repeated("pure"));
                }
                options.pure = true;
            }
            Meta::Path(path) if path.is_ident("client_static") => {
                if options.client_static {
                    return Err(repeated("client_static"));
                }
                options.client_static = true;
            }
            Meta::List(list) if list.path.is_ident("default") => {
                if !options.defaults.is_empty() {
                    return Err(repeated("default(...) list"));
                }
                options.defaults = list.tokens.clone();
            }
            other => return Err(syn::Error::new_spanned(other, EXPORT_OPTIONS)),
        }
    }
    if options.pure && options.client_static {
        return Err(syn::Error::new(
            Span::call_site(),
            "a pure export is synchronous and a client_static export is asynchronous; choose one",
        ));
    }
    if options.pure && options.target.is_some() {
        return Err(syn::Error::new(
            Span::call_site(),
            "a pure export selects its own targets; drop native_only or wasm_only",
        ));
    }
    if options.client_static && options.target.is_some() {
        return Err(syn::Error::new(
            Span::call_site(),
            "a client_static export is a Client static in every SDK; drop native_only or wasm_only",
        ));
    }
    if !options.defaults.is_empty() && !options.pure {
        return Err(syn::Error::new(
            Span::call_site(),
            "only pure functions accept one default(...) list",
        ));
    }
    Ok(options)
}

/// A record or enum keeps its UniFFI derive after this macro, so the markers
/// reach the library metadata.
fn require_uniffi_derive(attrs: &[syn::Attribute], name: &syn::Ident) -> syn::Result<()> {
    let derives_uniffi = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("derive"))
        .filter_map(|attr| {
            attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
                .ok()
        })
        .flatten()
        .any(|path| {
            path.segments
                .first()
                .is_some_and(|first| first.ident == "uniffi")
        });
    if derives_uniffi {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(
            name,
            "sdk_export must be the first attribute on a record or enum, above #[derive(uniffi::Record)] or #[derive(uniffi::Enum)] and every other derive",
        ))
    }
}

pub fn sdk_export(attr: TokenStream, input: TokenStream) -> syn::Result<TokenStream> {
    let options = parse_options(attr)?;
    let cfg = options.target.map(Target::cfg);
    let mut item: Item = syn::parse2(input)?;
    sdk_member::reject_written_markers(&item)?;
    if options.pure {
        match &mut item {
            Item::Fn(function) if function.sig.asyncness.is_none() => {
                push_marker(&mut function.attrs, PURE);
            }
            Item::Fn(function) => {
                return Err(syn::Error::new_spanned(
                    &function.sig,
                    "pure export must be synchronous",
                ));
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    item,
                    "pure export must be a free function",
                ));
            }
        }
    }
    if options.client_static {
        match &mut item {
            Item::Fn(function) if function.sig.asyncness.is_some() => {
                push_marker(&mut function.attrs, CLIENT_STATIC);
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    item,
                    "client_static needs an asynchronous free function",
                ));
            }
        }
    }
    let has_async = match &mut item {
        Item::Impl(item_impl) => {
            let mut has_async = false;
            for impl_item in &mut item_impl.items {
                if let ImplItem::Fn(function) = impl_item {
                    has_async |= function.sig.asyncness.is_some();
                    sdk_member::method(&mut function.attrs, &function.sig, options.target, true)?;
                    instrument(&mut function.attrs, &function.sig.output);
                }
            }
            has_async
        }
        Item::Fn(function) => {
            sdk_member::function(&mut function.attrs, &function.sig.ident)?;
            instrument(&mut function.attrs, &function.sig.output);
            function.sig.asyncness.is_some()
        }
        Item::Trait(item_trait) => {
            let mut has_async = false;
            for trait_item in &mut item_trait.items {
                if let TraitItem::Fn(function) = trait_item {
                    has_async |= function.sig.asyncness.is_some();
                    sdk_member::method(&mut function.attrs, &function.sig, options.target, false)?;
                    if function.default.is_some() {
                        instrument(&mut function.attrs, &function.sig.output);
                    }
                }
            }
            has_async
        }
        // The UniFFI derive exports a record or enum; this macro adds the
        // markers, the target's cfg, and the Debug of a redacted type.
        Item::Struct(item_struct) => {
            require_uniffi_derive(&item_struct.attrs, &item_struct.ident)?;
            let uniffi_error = sdk_member::derives_uniffi_error(&item_struct.attrs);
            let redacted =
                sdk_member::fields(&mut item_struct.fields, &item_struct.ident, uniffi_error)?;
            let debug = if redacted {
                let debug = sdk_member::redacted_debug(
                    &mut item_struct.attrs,
                    &item_struct.ident,
                    &item_struct.generics,
                )?;
                Some(quote!(#cfg #debug))
            } else {
                None
            };
            return Ok(quote!(#cfg #item_struct #debug));
        }
        Item::Enum(item_enum) => {
            require_uniffi_derive(&item_enum.attrs, &item_enum.ident)?;
            let debug = if sdk_member::variants(item_enum)? {
                let debug = sdk_member::redacted_debug(
                    &mut item_enum.attrs,
                    &item_enum.ident,
                    &item_enum.generics,
                )?;
                Some(quote!(#cfg #debug))
            } else {
                None
            };
            return Ok(quote!(#cfg #item_enum #debug));
        }
        _ => {
            return Err(syn::Error::new_spanned(
                item,
                "sdk_export requires an impl block, trait, function, record, or enum",
            ));
        }
    };

    let export = if options.pure {
        let pure_export = if options.defaults.is_empty() {
            quote!(uniffi::export)
        } else {
            let defaults = &options.defaults;
            quote!(uniffi::export(default(#defaults)))
        };
        quote! {
            #[cfg_attr(any(not(target_arch = "wasm32"), feature = "pure-only"), #pure_export)]
        }
    } else if has_async {
        quote! {
            #[cfg_attr(not(target_arch = "wasm32"), uniffi::export(async_runtime = "tokio"))]
            #[cfg_attr(target_arch = "wasm32", uniffi::export)]
        }
    } else {
        quote!(#[uniffi::export])
    };

    Ok(quote! {
        #cfg
        #export
        #item
    })
}

fn instrument(attrs: &mut Vec<syn::Attribute>, output: &ReturnType) {
    if attrs.iter().any(|attr| {
        attr.path().is_ident("instrument")
            || attr.path().segments.len() == 2
                && attr.path().segments[0].ident == "tracing"
                && attr.path().segments[1].ident == "instrument"
    }) {
        return;
    }

    let annotation: syn::Attribute = if returns_result(output) {
        syn::parse_quote!(#[tracing::instrument(err, skip_all)])
    } else {
        syn::parse_quote!(#[tracing::instrument(skip_all)])
    };
    attrs.push(annotation);
}

pub(crate) fn returns_result(output: &ReturnType) -> bool {
    fn is_result(ty: &Type) -> bool {
        match ty {
            Type::Path(path) => path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "Result"),
            Type::Group(group) => is_result(&group.elem),
            Type::Paren(paren) => is_result(&paren.elem),
            _ => false,
        }
    }

    matches!(output, ReturnType::Type(_, ty) if is_result(ty))
}

pub fn callback_error(attr: TokenStream, input: TokenStream) -> syn::Result<TokenStream> {
    if !attr.is_empty() {
        return Err(syn::Error::new_spanned(
            attr,
            "callback_error does not accept arguments",
        ));
    }

    let item: Item = syn::parse2(input)?;
    let (name, generics) = match &item {
        Item::Enum(item_enum) => (&item_enum.ident, &item_enum.generics),
        Item::Struct(item_struct) => (&item_struct.ident, &item_struct.generics),
        _ => {
            return Err(syn::Error::new_spanned(
                item,
                "callback_error requires an enum or struct",
            ));
        }
    };
    if !generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            generics,
            "callback_error requires a concrete type without generics",
        ));
    }

    Ok(quote! {
        #item

        const _: fn() = || {
            fn assert_from<T: ::core::convert::From<::uniffi::UnexpectedUniFFICallbackError>>() {}
            assert_from::<#name>();
        };
    })
}
