use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, Field, Ident, ItemStruct, Type, Visibility, parse_str};

/// Annotation configuration supplied by the caller (proc macro entry point).
///
/// This keeps `builder.rs` fully binding-agnostic — it has no knowledge of
/// NAPI, WASM, UniFFI, or any other binding system. The caller provides the
/// annotation tokens for each position.
pub struct AnnotationConfig {
    /// Annotation placed on the struct definition.
    /// e.g. `#[::napi_derive::napi]` or `#[derive(uniffi::Object)]`
    pub struct_ann: TokenStream,
    /// Annotation placed on the `impl` block containing the constructor.
    /// e.g. `#[::napi_derive::napi]` or `#[uniffi::export]`
    pub impl_ann: TokenStream,
    /// Annotation placed on the constructor method.
    /// e.g. `#[::napi_derive::napi(constructor)]` or `#[uniffi::constructor]`
    pub constructor_ann: TokenStream,
    /// Per-setter annotation. Receives the **setter method name** [`Ident`]
    /// (after prefix) so the caller can derive names (e.g. camelCase for WASM).
    pub setter_ann: fn(&Ident) -> TokenStream,
    /// Optional separate annotation for the setter `impl` block.
    /// When `Some`, setters are placed in a separate `impl` block with this
    /// annotation (e.g. no annotation for UniFFI, where `&mut self` setters
    /// can't live inside `#[uniffi::export]`). When `None`, setters share
    /// the same `impl` block as the constructor (used by NAPI and WASM).
    pub setter_impl_ann: Option<TokenStream>,
    /// Controls the setter method signature style.
    pub setter_style: SetterStyle,
    /// Prefix prepended to setter method names (e.g. `"set_"` for NAPI/WASM
    /// to avoid conflicts with auto-generated field property getters).
    /// Empty string means the setter name equals the field name.
    pub setter_prefix: &'static str,
}

/// Controls how setter methods receive and return `self`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetterStyle {
    /// `&mut self` + NAPI `This<'scope>` — injects the JS `this` object and
    /// returns it for chaining: `new Builder("x").setPort(8080).build()`.
    NapiThis,
    /// `mut self -> Self` — consuming pattern for wasm_bindgen.
    /// Enables JS chaining: `new Builder("x").setPort(8080).build()`.
    Consuming,
    /// `&mut self -> &mut Self` — Rust-side chaining for non-exported setters
    /// (UniFFI, tests).
    MutRefChain,
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
    let (impl_generics, ty_generics, where_clause) = struct_generics.split_for_impl();

    // Preserve non-macro struct-level attributes (e.g. #[allow(dead_code)]).
    let other_struct_attrs: Vec<&syn::Attribute> = input
        .attrs
        .iter()
        .filter(|a| !a.path().is_ident("builder"))
        .collect();

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

    let struct_attr = &config.struct_ann;

    let struct_def = quote! {
        #(#other_struct_attrs)*
        #struct_attr
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
    let setters: Vec<TokenStream> = parsed
        .iter()
        .filter(|f| matches!(f.mode, FieldMode::Optional | FieldMode::Default))
        .map(|f| {
            let field_ident = &f.ident;

            // Compute setter method name: prefix + field name (e.g. "set_flag").
            let setter_name = if config.setter_prefix.is_empty() {
                f.ident.clone()
            } else {
                Ident::new(
                    &format!("{}{}", config.setter_prefix, f.ident),
                    f.ident.span(),
                )
            };

            let setter_ann = (config.setter_ann)(&setter_name);

            let (param_ty, assign_expr) = match f.mode {
                FieldMode::Optional => {
                    let inner_ty = extract_option_inner(&f.ty).expect(
                        "optional field must be Option<T> (this was validated during parsing)",
                    );
                    (quote! { #inner_ty }, quote! { Some(#field_ident) })
                }
                FieldMode::Default => {
                    let ty = &f.ty;
                    (quote! { #ty }, quote! { #field_ident })
                }
                _ => unreachable!(),
            };

            // The parameter uses the field name so `self.field = field` reads naturally.
            match config.setter_style {
                SetterStyle::NapiThis => {
                    // NAPI `This<'scope>` pattern for JS chaining.
                    // `This` is injected by NAPI-RS; JS callers don't pass it.
                    // Returning `This` makes the setter chainable in JS.
                    quote! {
                        #setter_ann
                        pub fn #setter_name<'scope>(
                            &'scope mut self,
                            this: ::napi::bindgen_prelude::This<'scope>,
                            #field_ident: #param_ty,
                        ) -> ::napi::bindgen_prelude::This<'scope> {
                            self.#field_ident = #assign_expr;
                            this
                        }
                    }
                }
                SetterStyle::Consuming => {
                    // `mut self -> Self` for wasm_bindgen JS-side chaining.
                    quote! {
                        #setter_ann
                        pub fn #setter_name(mut self, #field_ident: #param_ty) -> Self {
                            self.#field_ident = #assign_expr;
                            self
                        }
                    }
                }
                SetterStyle::MutRefChain => {
                    // `&mut self -> &mut Self` for Rust-side chaining.
                    quote! {
                        #setter_ann
                        pub fn #setter_name(&mut self, #field_ident: #param_ty) -> &mut Self {
                            self.#field_ident = #assign_expr;
                            self
                        }
                    }
                }
            }
        })
        .collect();

    // 5. Combine ---------------------------------------------------------
    let impl_attr = &config.impl_ann;

    let impl_block = if let Some(setter_impl_attr) = &config.setter_impl_ann {
        // Split: constructor in one impl block, setters in another.
        // Used by UniFFI where `#[uniffi::export]` wraps methods in Arc<Self>,
        // making `&mut self` setters incompatible.
        quote! {
            #impl_attr
            impl #impl_generics #struct_ident #ty_generics #where_clause {
                #constructor
            }

            #setter_impl_attr
            impl #impl_generics #struct_ident #ty_generics #where_clause {
                #(#setters)*
            }
        }
    } else {
        // Single impl block: constructor + setters together (NAPI, WASM).
        quote! {
            #impl_attr
            impl #impl_generics #struct_ident #ty_generics #where_clause {
                #constructor
                #(#setters)*
            }
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

        if mode.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "duplicate #[builder(...)] attribute; each field may only have one",
            ));
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

    let mode = match mode {
        Some(m) => m,
        None if extract_option_inner(&ty).is_some() => FieldMode::Optional,
        None => {
            return Err(syn::Error::new_spanned(
                field,
                "field must have a #[builder(required | optional | default = \"..\" | skip)] attribute",
            ));
        }
    };

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
    let mut seen_alpha = false;
    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next && seen_alpha {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
            capitalize_next = false;
            seen_alpha = true;
        }
    }
    result
}
