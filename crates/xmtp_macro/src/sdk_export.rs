use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    ImplItem, Item, Meta, ReturnType, Token, TraitItem, Type, parse::Parser, punctuated::Punctuated,
};

pub fn sdk_export(attr: TokenStream, input: TokenStream) -> syn::Result<TokenStream> {
    let mut pure = false;
    let mut defaults = TokenStream::new();
    let arguments = Punctuated::<Meta, Token![,]>::parse_terminated.parse2(attr)?;
    let mut arguments = arguments.into_iter();
    let target = if arguments.len() == 0 {
        None
    } else {
        let first = arguments.next().unwrap();
        let Meta::Path(path) = first else {
            return Err(syn::Error::new_spanned(
                first,
                "sdk_export requires a target name",
            ));
        };
        let target = path
            .get_ident()
            .ok_or_else(|| syn::Error::new_spanned(&path, "sdk_export requires a target name"))?;
        match target.to_string().as_str() {
            "native_only" => Some(quote!(#[cfg(not(target_arch = "wasm32"))])),
            "wasm_only" => Some(quote!(#[cfg(target_arch = "wasm32")])),
            "pure" => {
                pure = true;
                None
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    target,
                    "sdk_export accepts only native_only, wasm_only, or pure",
                ));
            }
        }
    };

    for argument in arguments {
        match argument {
            Meta::List(value) if pure && value.path.is_ident("default") && defaults.is_empty() => {
                defaults = value.tokens;
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "only pure functions accept one default(...) list",
                ));
            }
        }
    }
    let pure_export = if defaults.is_empty() {
        quote!(uniffi::export)
    } else {
        quote!(uniffi::export(default(#defaults)))
    };
    let mut item: Item = syn::parse2(input)?;
    if pure {
        match &mut item {
            Item::Fn(function) if function.sig.asyncness.is_none() => {
                function
                    .attrs
                    .push(syn::parse_quote!(#[doc = "@xmtp-pure"]));
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
    let has_async = match &mut item {
        Item::Impl(item_impl) => {
            let mut has_async = false;
            for impl_item in &mut item_impl.items {
                if let ImplItem::Fn(method) = impl_item {
                    has_async |= method.sig.asyncness.is_some();
                    instrument(&mut method.attrs, &method.sig.output);
                }
            }
            has_async
        }
        Item::Fn(function) => {
            instrument(&mut function.attrs, &function.sig.output);
            function.sig.asyncness.is_some()
        }
        Item::Trait(item_trait) => {
            let mut has_async = false;
            for trait_item in &mut item_trait.items {
                if let TraitItem::Fn(method) = trait_item {
                    has_async |= method.sig.asyncness.is_some();
                    if method.default.is_some() {
                        instrument(&mut method.attrs, &method.sig.output);
                    }
                }
            }
            has_async
        }
        _ => {
            return Err(syn::Error::new_spanned(
                item,
                "sdk_export requires an impl block, trait, or function",
            ));
        }
    };

    let export = if pure {
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
        #target
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

fn returns_result(output: &ReturnType) -> bool {
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
