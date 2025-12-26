//! Procedural macros for WASM bindings.
//!
//! This crate provides the `#[wasm_bindgen_enum]` attribute macro for defining
//! enums that work seamlessly with wasm-bindgen and serde, producing TypeScript
//! const enums with numeric values.

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemEnum, parse_macro_input};

/// Attribute macro that transforms an enum into a wasm-bindgen compatible enum
/// with numeric serde serialization.
///
/// This generates:
/// - `#[wasm_bindgen]` attribute for TypeScript enum generation
/// - `#[repr(u8)]` for numeric representation
/// - `#[derive(Clone, Copy, Debug, PartialEq, Eq)]`
/// - `Serialize` impl that serializes as u8
/// - `Deserialize` impl that deserializes from u8
///
/// # Example
///
/// ```ignore
/// #[wasm_bindgen_numbered_enum]
/// pub enum MyEnum {
///     VariantA = 0,
///     VariantB = 1,
/// }
/// ```
///
/// You can also add additional derives:
///
/// ```ignore
/// #[wasm_bindgen_numbered_enum]
/// #[derive(Hash)]
/// pub enum MyEnum {
///     VariantA = 0,
///     VariantB = 1,
/// }
/// ```
#[proc_macro_attribute]
pub fn wasm_bindgen_numbered_enum(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemEnum);

    let vis = &input.vis;
    let name = &input.ident;
    let name_str = name.to_string();

    // Collect existing attributes (like additional derives)
    let existing_attrs: Vec<_> = input.attrs.iter().collect();

    // Collect variants with their discriminants and attributes
    let variants: Vec<_> = input
        .variants
        .iter()
        .map(|v| {
            let variant_name = &v.ident;
            let variant_attrs = &v.attrs;
            let discriminant = v
                .discriminant
                .as_ref()
                .map(|(_, expr)| quote! { #expr })
                .expect("All variants must have explicit discriminants");
            (variant_name, variant_attrs, discriminant)
        })
        .collect();

    // Generate enum variants for the definition
    let enum_variants = variants.iter().map(|(name, attrs, disc)| {
        quote! {
            #(#attrs)*
            #name = #disc
        }
    });

    // Generate match arms for deserialization
    let deser_arms = variants.iter().map(|(variant_name, _, disc)| {
        quote! {
            #disc => Ok(Self::#variant_name)
        }
    });

    let expanded = quote! {
        #[::wasm_bindgen::prelude::wasm_bindgen]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u8)]
        #(#existing_attrs)*
        #vis enum #name {
            #(#enum_variants),*
        }

        impl ::serde::Serialize for #name {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_u8(*self as u8)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for #name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = <u8 as ::serde::Deserialize>::deserialize(deserializer)?;
                match value {
                    #(#deser_arms,)*
                    v => Err(::serde::de::Error::custom(format!(
                        concat!("invalid ", #name_str, " value: {}"), v
                    ))),
                }
            }
        }
    };

    TokenStream::from(expanded)
}
