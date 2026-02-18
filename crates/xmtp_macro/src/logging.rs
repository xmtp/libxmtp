use std::fmt::Display;

use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};
use syn::{
    Attribute, Expr, Ident, LitStr, Meta, Path, Token, Variant,
    parse::{Parse, ParseStream},
};

pub(crate) struct Field {
    pub(crate) name: Ident,
    pub(crate) sigil: Option<char>,
    pub(crate) value: Option<Expr>,
}

pub(crate) enum LogLevel {
    Info,
    Warn,
    Error,
}
impl Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

pub(crate) struct LogEventInput {
    pub(crate) event: Path,
    pub(crate) level: LogLevel,
    pub(crate) fields: Vec<Field>,
    pub(crate) installation_id: Expr,
}

impl Parse for LogEventInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let event: Path = input.parse()?;
        input.parse::<Token![,]>()?;

        // Parse installation_id as the second argument
        let installation_id: Expr = input.parse()?;
        let mut level = LogLevel::Info;
        let mut fields = Vec::new();

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }

            let sigil = if input.peek(Token![%]) {
                input.parse::<Token![%]>()?;
                Some('%')
            } else if input.peek(Token![?]) {
                input.parse::<Token![?]>()?;
                Some('?')
            } else if input.peek(Token![#]) {
                input.parse::<Token![#]>()?;
                Some('#')
            } else if input.peek(Token![$]) {
                input.parse::<Token![$]>()?;
                Some('$')
            } else {
                None
            };

            let name: Ident = input.parse()?;

            let (sigil, value) = if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;

                let value_sigil = if input.peek(Token![%]) {
                    input.parse::<Token![%]>()?;
                    Some('%')
                } else if input.peek(Token![?]) {
                    input.parse::<Token![?]>()?;
                    Some('?')
                } else if input.peek(Token![#]) {
                    input.parse::<Token![#]>()?;
                    Some('#')
                } else if input.peek(Token![$]) {
                    input.parse::<Token![$]>()?;
                    Some('$')
                } else {
                    None
                };

                let expr: Expr = input.parse()?;
                (sigil.or(value_sigil), Some(expr))
            } else {
                (sigil, None)
            };

            if name == "level" {
                let Some(value) = value else {
                    return syn::Result::Err(syn::Error::new(
                        input.span(),
                        "`level` is missing value.",
                    ));
                };
                // Extract the identifier from the expression
                let level_str = match &value {
                    Expr::Path(expr_path) if expr_path.path.get_ident().is_some() => {
                        expr_path.path.get_ident().unwrap().to_string()
                    }
                    _ => {
                        return syn::Result::Err(syn::Error::new_spanned(
                            &value,
                            "level must be an identifier: info, warn, or error",
                        ));
                    }
                };
                level = match level_str.as_str() {
                    "info" => LogLevel::Info,
                    "warn" => LogLevel::Warn,
                    "error" => LogLevel::Error,
                    val => {
                        return syn::Result::Err(syn::Error::new_spanned(
                            &value,
                            format!(
                                "{val} is an invalid value for `level`. \
                                 Valid values are `info`, `warn`, `error`."
                            ),
                        ));
                    }
                };
                continue;
            }

            // Handle # sigil for short_hex transformation
            // Keep sigil as '#' so context formatting can quote the value
            // Auto-apply # for known byte-like field names (e.g. group_id)
            let name_str = name.to_string();
            let short_hex_fields = &["group_id", "installation_id", "epoch_auth"];
            if sigil == Some('#')
                || (sigil.is_none()
                    && short_hex_fields
                        .iter()
                        .any(|f| name_str.contains(f) && !name_str.contains("full")))
            {
                let value_expr = value.unwrap_or_else(|| {
                    let name_clone = name.clone();
                    syn::parse_quote!(#name_clone)
                });
                let transformed_value: Expr = syn::parse_quote! {
                    {
                        use xmtp_proto::ShortHex;
                        #value_expr.short_hex()
                    }
                };
                fields.push(Field {
                    name,
                    sigil: Some('#'),
                    value: Some(transformed_value),
                });
                continue;
            }

            // Handle $ sigil for serde_json::to_string transformation
            if sigil == Some('$') {
                let value_expr = value.unwrap_or_else(|| {
                    let name_clone = name.clone();
                    syn::parse_quote!(#name_clone)
                });
                let transformed_value: Expr = syn::parse_quote! {
                    ::serde_json::to_string(&(#value_expr)).unwrap_or_else(|e| format!("<json error: {e}>"))
                };
                fields.push(Field {
                    name,
                    sigil: Some('$'),
                    value: Some(transformed_value),
                });
                continue;
            }

            fields.push(Field { name, sigil, value });
        }

        Ok(LogEventInput {
            event,
            level,
            fields,
            installation_id,
        })
    }
}

impl Field {
    pub(crate) fn to_tracing_tokens(&self) -> TokenStream2 {
        let name = &self.name;
        let value = self
            .value
            .as_ref()
            .map(|e| e.to_token_stream())
            .unwrap_or_else(|| name.to_token_stream());

        match self.sigil {
            Some('%') | Some('#') | Some('$') => quote! { #name = %#value },
            Some('?') => quote! { #name = ?#value },
            _ => quote! { #name = #value },
        }
    }

    pub(crate) fn value_tokens(&self) -> TokenStream2 {
        self.value
            .as_ref()
            .map(|e| e.to_token_stream())
            .unwrap_or_else(|| self.name.to_token_stream())
    }
}

pub(crate) fn get_doc_comment(variant: &Variant) -> Result<String, syn::Error> {
    let doc_comment = variant.attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("doc") {
            return None;
        }
        if let Meta::NameValue(nv) = &attr.meta
            && let Expr::Lit(expr_lit) = &nv.value
            && let syn::Lit::Str(s) = &expr_lit.lit
        {
            return Some(s.value().trim().to_string());
        }
        None
    });

    doc_comment.ok_or_else(|| syn::Error::new_spanned(variant, "Doc comment is required."))
}

pub(crate) fn get_context_fields(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .find_map(|attr| {
            if !attr.path().is_ident("context") {
                return None;
            }
            let mut fields = Vec::new();
            let _ = attr.parse_nested_meta(|meta| {
                if let Some(ident) = meta.path.get_ident() {
                    let ident = ident.to_string();

                    if ident == "icon" {
                        return Ok(());
                    }

                    fields.push(ident);
                }
                Ok(())
            });
            Some(fields)
        })
        .unwrap_or_default()
}

pub(crate) fn get_icon(attrs: &[Attribute]) -> Option<String> {
    let mut icon = None;
    for attr in attrs {
        if !attr.path().is_ident("context") {
            continue;
        }

        let _ = attr.parse_nested_meta(|meta| {
            let Some(ident) = meta.path.get_ident() else {
                return Ok(());
            };
            if ident != "icon" {
                return Ok(());
            }

            let value: LitStr = meta.value()?.parse()?;
            icon = Some(value.value());

            Ok(())
        });
    }

    icon
}
