use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};
use syn::{
    Attribute, Expr, Ident, Meta, Path, Token, Variant,
    parse::{Parse, ParseStream},
};

pub(crate) struct Field {
    pub(crate) name: Ident,
    pub(crate) sigil: Option<char>,
    pub(crate) value: Option<Expr>,
}

pub(crate) struct LogEventInput {
    pub(crate) event: Path,
    pub(crate) fields: Vec<Field>,
}

impl Parse for LogEventInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let event: Path = input.parse()?;
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
                } else {
                    None
                };

                let expr: Expr = input.parse()?;
                (sigil.or(value_sigil), Some(expr))
            } else {
                (sigil, None)
            };

            fields.push(Field { name, sigil, value });
        }

        Ok(LogEventInput { event, fields })
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
            Some('%') => quote! { #name = %#value },
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
        if let Meta::NameValue(nv) = &attr.meta {
            if let Expr::Lit(expr_lit) = &nv.value {
                if let syn::Lit::Str(s) = &expr_lit.lit {
                    return Some(s.value().trim().to_string());
                }
            }
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
                    fields.push(ident.to_string());
                }
                Ok(())
            });
            Some(fields)
        })
        .unwrap_or_default()
}
