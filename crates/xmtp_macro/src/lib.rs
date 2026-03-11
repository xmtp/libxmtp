extern crate proc_macro;

mod async_trait;
mod builder;
mod builders;
mod error_code;
mod log_macros;
mod logging;
mod test_macro;

#[cfg(test)]
mod builder_test;

/// A proc macro attribute that wraps the input in an `async_trait` implementation,
/// delegating to the appropriate `async_trait` implementation based on the target architecture.
///
/// On wasm32 architecture, it delegates to `async_trait::async_trait(?Send)`.
/// On all other architectures, it delegates to `async_trait::async_trait`.
#[proc_macro_attribute]
pub fn async_trait(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    async_trait::async_trait(attr, input)
}

/// Attribute macro that generates a NAPI-annotated builder pattern for a struct.
///
/// Each field must be annotated with one of:
/// - `#[builder(required)]` — passed in the constructor; no setter generated
/// - `#[builder(optional)]` — field type must be `Option<T>`; setter takes `T`, wraps in `Some`
/// - `#[builder(default = "expr")]` — has a default value; setter takes the full type
/// - `#[builder(skip)]` — no setter; initialized via `Default::default()`
///
/// The macro generates a `new()` constructor (with all required fields as parameters)
/// and fluent setters for optional/default fields. The `build()` method is NOT generated;
/// implement it manually.
///
/// # Example
///
/// ```ignore
/// #[napi_builder]
/// pub struct FooBuilder {
///     #[builder(required)]
///     name: String,
///     #[builder(optional)]
///     desc: Option<String>,
///     #[builder(default = "42")]
///     count: u32,
///     #[builder(skip)]
///     internal: Vec<u8>,
/// }
/// ```
#[proc_macro_attribute]
pub fn napi_builder(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    builders::napi_builder(attr, input)
}

/// Attribute macro that generates a wasm_bindgen-annotated builder pattern for a struct.
///
/// Behaves identically to [`napi_builder`] but emits `#[wasm_bindgen]` annotations
/// instead of `#[napi]`, and generates `js_name = camelCase` attributes on setters.
///
/// See [`napi_builder`] for field attribute documentation.
#[proc_macro_attribute]
pub fn wasm_builder(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    builders::wasm_builder(attr, input)
}

/// Attribute macro that generates a UniFFI-annotated builder pattern for a struct.
///
/// Emits `#[derive(uniffi::Object)]` on the struct and `#[uniffi::export]` on the
/// impl block. UniFFI annotates the impl block as a whole rather than individual
/// methods, so `constructor_ann` and `setter_ann` are empty.
///
/// See [`napi_builder`] for field attribute documentation.
#[proc_macro_attribute]
pub fn uniffi_builder(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    builders::uniffi_builder(attr, input)
}

/// A test macro that delegates to the appropriate test framework based on the target architecture.
///
/// On wasm32 architecture, it delegates to `wasm_bindgen_test::wasm_bindgen_test`.
/// On all other architectures, it delegates to `tokio::test`.
///
/// When using with 'rstest', ensure any other test invocations come after rstest invocation.
/// # Example
///
/// ```ignore
/// #[test]
/// async fn test_something() {
///     assert_eq!(2 + 2, 4);
/// }
/// ```
#[proc_macro_attribute]
pub fn test(
    attr: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    test_macro::test(attr, body)
}

#[proc_macro_attribute]
pub fn build_logging_metadata(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    log_macros::build_logging_metadata(attr, item)
}

#[proc_macro]
pub fn log_event(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    log_macros::log_event(input)
}

/// Derive macro for the `ErrorCode` trait.
///
/// Automatically generates an `error_code()` implementation that returns
/// `"TypeName::VariantName"` for each enum variant, or `"TypeName"` for structs.
///
/// # Example
///
/// ```ignore
/// use xmtp_common::ErrorCode;
///
/// #[derive(Debug, thiserror::Error, ErrorCode)]
/// pub enum GroupError {
///     #[error("Group not found")]
///     NotFound,  // Returns "GroupError::NotFound"
///
///     #[error("Storage error: {0}")]
///     #[error_code(inherit)]  // Delegates to StorageError::error_code()
///     Storage(#[from] StorageError),
/// }
/// ```
///
/// # Attributes
///
/// - `#[error_code(inherit)]` - Delegate to the inner error's `error_code()` method.
///   Use this for single-field variants that wrap another error implementing `ErrorCode`.
///
/// - `#[error_code(remote = "path::Type")]` - Implement `ErrorCode` for a remote type.
///   The derived item should mirror the remote type's shape. Default codes use the derived
///   item's type name, so keep it aligned with the remote type's name unless overridden.
///
/// - `#[error_code("CustomCode")]` - Override the generated code with a custom value.
///   Use this to maintain backwards compatibility when renaming variants.
///
/// # Example: Custom Code for Backwards Compatibility
///
/// ```ignore
/// #[derive(Debug, thiserror::Error, ErrorCode)]
/// pub enum MyError {
///     // Renamed from "OldName" but keeps the old error code
///     #[error("new name")]
///     #[error_code("MyError::OldName")]
///     NewName,
/// }
/// ```
#[derive(Default)]
struct ErrorCodeAttr {
    /// Custom code override: #[error_code("CustomCode")]
    code: Option<String>,
    /// Inherit from inner error: #[error_code(inherit)]
    inherit: bool,
    /// Implement for a remote type path: #[error_code(remote = "path::Type")]
    remote: Option<Path>,
    /// Mark as internal (not surfaced to SDK consumers): #[error_code(internal)]
    internal: bool,
}

impl ErrorCodeAttr {
    fn parse(attrs: &[syn::Attribute]) -> Self {
        let mut result = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("error_code") {
                continue;
            }

            // Try parsing #[error_code("CustomCode")]
            if let Ok(lit) = attr.parse_args::<syn::LitStr>() {
                result.code = Some(lit.value());
                continue;
            }

            // Try parsing #[error_code(inherit)] or #[error_code(remote = "path::Type")]
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("inherit") {
                    result.inherit = true;
                    Ok(())
                } else if meta.path.is_ident("internal") {
                    result.internal = true;
                    Ok(())
                } else if meta.path.is_ident("remote") {
                    let value = meta.value()?;
                    let lit: syn::LitStr = value.parse()?;
                    let path = lit
                        .parse::<Path>()
                        .map_err(|err| meta.error(err.to_string()))?;
                    result.remote = Some(path);
                    Ok(())
                } else {
                    Err(meta.error(
                        "expected `inherit`, `internal`, `remote = \"path::Type\"`, or a string literal",
                    ))
                }
            });
        }

        result
    }
}

#[proc_macro_derive(ErrorCode, attributes(error_code))]
pub fn derive_error_code(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let container_attr = ErrorCodeAttr::parse(&input.attrs);
    let name_str = container_attr
        .remote
        .as_ref()
        .and_then(|path| {
            path.segments
                .last()
                .map(|segment| segment.ident.to_string())
        })
        .unwrap_or_else(|| name.to_string());
    let target = container_attr
        .remote
        .clone()
        .unwrap_or_else(|| syn::parse_quote!(#name));

    let is_remote = container_attr.remote.is_some();

    let expanded = match &input.data {
        Data::Enum(data_enum) => {
            let code_arms = data_enum.variants.iter().map(|variant| {
                let variant_name = &variant.ident;
                let default_code = format!("{}::{}", name_str, variant_name);
                let attr = ErrorCodeAttr::parse(&variant.attrs);

                if attr.inherit {
                    // For inherited errors, delegate to the inner error
                    match &variant.fields {
                        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                            quote! {
                                Self::#variant_name(e) => e.error_code(),
                            }
                        }
                        Fields::Named(fields) if fields.named.len() == 1 => {
                            let field_name = fields.named.first().unwrap().ident.as_ref().unwrap();
                            quote! {
                                Self::#variant_name { #field_name } => #field_name.error_code(),
                            }
                        }
                        _ => {
                            let span = variant_name.span();
                            quote_spanned! {span=>
                                compile_error!("#[error_code(inherit)] requires exactly one field");
                            }
                        }
                    }
                } else {
                    // Require doc comments on non-inherited, non-remote variants
                    if !is_remote {
                        let has_doc = variant.attrs.iter().any(|a| a.path().is_ident("doc"));
                        if !has_doc {
                            let msg = format!(
                                "Missing doc comment on error variant `{}::{}`. \
                                 All ErrorCode variants require a `///` doc comment \
                                 describing the error.",
                                name_str, variant_name
                            );
                            let span = variant_name.span();
                            return quote_spanned! {span=>
                                compile_error!(#msg);
                            };
                        }
                    }

                    // Use custom code if provided, otherwise use default
                    let code = attr.code.unwrap_or(default_code);

                    // Generate pattern based on fields
                    match &variant.fields {
                        Fields::Unit => {
                            quote! {
                                Self::#variant_name => #code,
                            }
                        }
                        Fields::Unnamed(_) => {
                            quote! {
                                Self::#variant_name(..) => #code,
                            }
                        }
                        Fields::Named(_) => {
                            quote! {
                                Self::#variant_name { .. } => #code,
                            }
                        }
                    }
                }
            });

            quote! {
                impl xmtp_common::ErrorCode for #target {
                    fn error_code(&self) -> &'static str {
                        match self {
                            #(#code_arms)*
                        }
                    }
                }
            }
        }
        Data::Struct(_) => {
            // Require doc comments on non-remote structs
            if !is_remote {
                let has_doc = input.attrs.iter().any(|a| a.path().is_ident("doc"));
                if !has_doc {
                    let msg = format!(
                        "Missing doc comment on error struct `{}`. \
                         All ErrorCode types require a `///` doc comment \
                         describing the error.",
                        name_str
                    );
                    return syn::Error::new_spanned(&input.ident, msg)
                        .to_compile_error()
                        .into();
                }
            }

            // Check for custom code on struct
            let code = container_attr.code.unwrap_or_else(|| name_str.clone());

            quote! {
                impl xmtp_common::ErrorCode for #target {
                    fn error_code(&self) -> &'static str {
                        #code
                    }
                }
            }
        }
        Data::Union(_) => {
            return syn::Error::new_spanned(&input, "ErrorCode cannot be derived for unions")
                .to_compile_error()
                .into();
        }
    };

    expanded.into()
}
