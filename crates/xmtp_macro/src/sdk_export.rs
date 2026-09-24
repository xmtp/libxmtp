use proc_macro2::TokenStream;
use quote::quote;
use syn::{ImplItem, Item, ReturnType, Type, Visibility};

pub fn sdk_export(attr: TokenStream, input: TokenStream) -> syn::Result<TokenStream> {
    let target = if attr.is_empty() {
        None
    } else {
        let target: syn::Ident = syn::parse2(attr)?;
        match target.to_string().as_str() {
            "native_only" => Some(quote!(#[cfg(not(target_arch = "wasm32"))])),
            "wasm_only" => Some(quote!(#[cfg(target_arch = "wasm32")])),
            _ => {
                return Err(syn::Error::new_spanned(
                    target,
                    "sdk_export accepts only native_only or wasm_only",
                ));
            }
        }
    };

    let mut item: Item = syn::parse2(input)?;
    match &mut item {
        Item::Impl(item_impl) => {
            for impl_item in &mut item_impl.items {
                if let ImplItem::Fn(method) = impl_item
                    && matches!(method.vis, Visibility::Public(_))
                {
                    instrument(&mut method.attrs, &method.sig.output);
                }
            }
        }
        Item::Fn(function) => instrument(&mut function.attrs, &function.sig.output),
        _ => {
            return Err(syn::Error::new_spanned(
                item,
                "sdk_export requires an impl block or a function",
            ));
        }
    }

    Ok(quote! {
        #target
        #[cfg_attr(not(target_arch = "wasm32"), uniffi::export(async_runtime = "tokio"))]
        #[cfg_attr(target_arch = "wasm32", uniffi::export)]
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
