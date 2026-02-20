use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, Field, Ident, ItemStruct, Type, Visibility, parse_str};

/// Annotation configuration supplied by the caller (proc macro entry point).
///
/// This keeps `builder.rs` fully binding-agnostic — it has no knowledge of
/// NAPI, WASM, or any other binding system. The caller provides the
/// annotation tokens for each position.
pub struct AnnotationConfig {
    /// Annotation placed on the struct definition **and** the `impl` block.
    /// e.g. `#[::napi_derive::napi]`
    pub binding_ann: TokenStream,
    /// Annotation placed on the constructor method.
    /// e.g. `#[::napi_derive::napi(constructor)]`
    pub constructor_ann: TokenStream,
    /// Per-setter annotation. Receives the field [`Ident`] so that the
    /// caller can derive names (e.g. camelCase for WASM).
    pub setter_ann: fn(&Ident) -> TokenStream,
}

/// How a field participates in the builder pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldMode {
    /// Passed as a constructor parameter; no setter generated.
    Required,
    /// Setter takes inner `T` (field must be `Option<T>`), wraps in `Some(T)`.
    /// Initialized to `None`.
    Optional,
    /// Has a default expression. Setter takes the full type. Initialized to
    /// the given expression.
    Default,
    /// No setter. Initialized via `Default::default()`.
    Skip,
}

/// Parsed representation of a single field inside the builder struct.
struct BuilderField {
    ident: Ident,
    ty: Type,
    mode: FieldMode,
    default_expr: Option<Expr>,
    vis: Visibility,
    /// Other attributes on the field that are NOT `#[builder(...)]`.
    other_attrs: Vec<syn::Attribute>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn expand_builder(input: ItemStruct, config: AnnotationConfig) -> syn::Result<TokenStream> {
    let fields = match &input.fields {
        syn::Fields::Named(named) => &named.named,
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "builder macros only support structs with named fields",
            ));
        }
    };

    // 1. Parse every field -----------------------------------------------
    let parsed: Vec<BuilderField> = fields
        .iter()
        .map(parse_field)
        .collect::<syn::Result<Vec<_>>>()?;

    // 2. Build the output struct (strip #[builder(...)] attrs) -----------
    let struct_vis = &input.vis;
    let struct_ident = &input.ident;
    let struct_generics = &input.generics;

    let cleaned_fields: Vec<TokenStream> = parsed
        .iter()
        .map(|f| {
            let ident = &f.ident;
            let ty = &f.ty;
            let vis = &f.vis;
            let other_attrs = &f.other_attrs;
            quote! {
                #(#other_attrs)*
                #vis #ident: #ty
            }
        })
        .collect();

    let binding_attr = &config.binding_ann;

    let struct_def = quote! {
        #binding_attr
        #struct_vis struct #struct_ident #struct_generics {
            #(#cleaned_fields),*
        }
    };

    // 3. Constructor -----------------------------------------------------
    let required: Vec<&BuilderField> = parsed
        .iter()
        .filter(|f| f.mode == FieldMode::Required)
        .collect();

    let ctor_params: Vec<TokenStream> = required
        .iter()
        .map(|f| {
            let ident = &f.ident;
            let ty = &f.ty;
            quote! { #ident: #ty }
        })
        .collect();

    let init_fields: Vec<TokenStream> = parsed
        .iter()
        .map(|f| {
            let ident = &f.ident;
            match f.mode {
                FieldMode::Required => quote! { #ident },
                FieldMode::Optional => quote! { #ident: None },
                FieldMode::Default => {
                    let expr = f.default_expr.as_ref().expect("default must have expr");
                    quote! { #ident: #expr }
                }
                FieldMode::Skip => quote! { #ident: Default::default() },
            }
        })
        .collect();

    let ctor_annotation = &config.constructor_ann;

    let constructor = quote! {
        #ctor_annotation
        pub fn new(#(#ctor_params),*) -> Self {
            Self {
                #(#init_fields),*
            }
        }
    };

    // 4. Setters ---------------------------------------------------------
    // Setters use `&mut self` (no return) so they can be exported to JS/WASM.
    // NAPI and wasm_bindgen cannot handle consuming-self or `-> &mut Self`.
    let setters: Vec<TokenStream> = parsed
        .iter()
        .filter(|f| matches!(f.mode, FieldMode::Optional | FieldMode::Default))
        .map(|f| {
            let ident = &f.ident;
            let setter_ann = (config.setter_ann)(&f.ident);

            match f.mode {
                FieldMode::Optional => {
                    let inner_ty = extract_option_inner(&f.ty).expect(
                        "optional field must be Option<T> (this was validated during parsing)",
                    );
                    quote! {
                        #setter_ann
                        pub fn #ident(&mut self, #ident: #inner_ty) {
                            self.#ident = Some(#ident);
                        }
                    }
                }
                FieldMode::Default => {
                    let ty = &f.ty;
                    quote! {
                        #setter_ann
                        pub fn #ident(&mut self, #ident: #ty) {
                            self.#ident = #ident;
                        }
                    }
                }
                _ => unreachable!(),
            }
        })
        .collect();

    // 5. Combine ---------------------------------------------------------
    let impl_block = quote! {
        #binding_attr
        impl #struct_generics #struct_ident #struct_generics {
            #constructor
            #(#setters)*
        }
    };

    Ok(quote! {
        #struct_def
        #impl_block
    })
}

// ---------------------------------------------------------------------------
// Field parsing
// ---------------------------------------------------------------------------

fn parse_field(field: &Field) -> syn::Result<BuilderField> {
    let ident = field
        .ident
        .clone()
        .ok_or_else(|| syn::Error::new_spanned(field, "expected a named field"))?;

    let ty = field.ty.clone();
    let vis = field.vis.clone();

    // Separate `#[builder(...)]` attributes from everything else.
    let mut mode: Option<FieldMode> = None;
    let mut default_expr: Option<Expr> = None;
    let mut other_attrs: Vec<syn::Attribute> = Vec::new();

    for attr in &field.attrs {
        if !attr.path().is_ident("builder") {
            other_attrs.push(attr.clone());
            continue;
        }

        // Parse the contents of #[builder(...)].
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("required") {
                mode = Some(FieldMode::Required);
                Ok(())
            } else if meta.path.is_ident("optional") {
                mode = Some(FieldMode::Optional);
                Ok(())
            } else if meta.path.is_ident("skip") {
                mode = Some(FieldMode::Skip);
                Ok(())
            } else if meta.path.is_ident("default") {
                mode = Some(FieldMode::Default);
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                let expr: Expr =
                    parse_str(&lit.value()).map_err(|e| syn::Error::new(lit.span(), e))?;
                default_expr = Some(expr);
                Ok(())
            } else {
                Err(meta.error("expected one of: required, optional, default, skip"))
            }
        })?;
    }

    let mode = mode.ok_or_else(|| {
        syn::Error::new_spanned(
            field,
            "field must have a #[builder(required | optional | default = \"..\" | skip)] attribute",
        )
    })?;

    // Validate optional fields are actually `Option<T>`.
    if mode == FieldMode::Optional && extract_option_inner(&ty).is_none() {
        return Err(syn::Error::new_spanned(
            &ty,
            "fields marked #[builder(optional)] must have type Option<T>",
        ));
    }

    Ok(BuilderField {
        ident,
        ty,
        mode,
        default_expr,
        vis,
        other_attrs,
    })
}

// ---------------------------------------------------------------------------
// Type helpers
// ---------------------------------------------------------------------------

/// Given `Option<T>`, returns `Some(T)`. Returns `None` for any other type.
fn extract_option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };

    // Check if the last segment is `Option`.
    let last = type_path.path.segments.last()?;
    if last.ident != "Option" {
        return None;
    }

    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };

    // Extract the first (and only) generic argument.
    if args.args.len() != 1 {
        return None;
    }

    match args.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// camelCase conversion (public so callers can use it for WASM js_name)
// ---------------------------------------------------------------------------

pub fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("my_field_name"), "myFieldName");
        assert_eq!(to_camel_case("desc"), "desc");
        assert_eq!(to_camel_case("a_b_c"), "aBC");
        assert_eq!(to_camel_case("already"), "already");
    }
}
