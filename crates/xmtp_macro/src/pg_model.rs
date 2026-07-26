//! `#[derive(PgModel)]` — the async storage track's row mapping.
//!
//! The sync track gets its column mapping from `diesel::table!` plus the
//! `Queryable`/`Insertable` derives. The async track has neither: `schema.rs` is
//! sync-only, so nothing there checks a `SELECT` against the schema at compile
//! time. This derive supplies the missing half — a column list and a by-name
//! `FromRow` — from the struct's own fields.
//!
//! Field-driven is the point. Because both backends read the same fields, the
//! two column lists cannot drift apart; the only thing restated is the table
//! name. And decoding by name rather than by position removes a whole failure
//! class: a hand-written mapper pairs a `SELECT` string with positional
//! `try_get` indices, and nothing but a test notices when the two fall out of
//! step.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, LitStr};

#[derive(Default)]
struct FieldOpts {
    /// Not a column: excluded from `COLUMNS` and filled with `Default::default()`.
    skip: bool,
    /// Column name, when it differs from the field name.
    rename: Option<String>,
}

fn struct_table(ast: &DeriveInput) -> syn::Result<String> {
    let mut table = None;
    for attr in &ast.attrs {
        if !attr.path().is_ident("xmtp") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("table") {
                let value: LitStr = meta.value()?.parse()?;
                table = Some(value.value());
                Ok(())
            } else {
                Err(meta.error("unrecognized `xmtp` option; expected `table = \"...\"`"))
            }
        })?;
    }
    table.ok_or_else(|| {
        syn::Error::new_spanned(
            &ast.ident,
            "PgModel requires #[xmtp(table = \"...\")] naming the Postgres table or view",
        )
    })
}

fn field_opts(field: &syn::Field) -> syn::Result<FieldOpts> {
    let mut opts = FieldOpts::default();
    for attr in &field.attrs {
        if !attr.path().is_ident("xmtp") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                opts.skip = true;
                Ok(())
            } else if meta.path.is_ident("rename") {
                let value: LitStr = meta.value()?.parse()?;
                opts.rename = Some(value.value());
                Ok(())
            } else {
                Err(meta.error("unrecognized `xmtp` option; expected `skip` or `rename = \"...\"`"))
            }
        })?;
    }
    Ok(opts)
}

pub fn derive_pg_model(input: TokenStream) -> syn::Result<TokenStream> {
    let ast: DeriveInput = syn::parse2(input)?;
    let name = &ast.ident;
    let table = struct_table(&ast)?;

    let Data::Struct(data) = &ast.data else {
        return Err(syn::Error::new_spanned(
            name,
            "PgModel can only be derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            name,
            "PgModel requires named fields",
        ));
    };

    let mut columns: Vec<String> = Vec::new();
    let mut decode: Vec<TokenStream> = Vec::new();
    let mut defaulted: Vec<TokenStream> = Vec::new();

    for field in &fields.named {
        let ident = field.ident.clone().expect("named field");
        let opts = field_opts(field)?;
        if opts.skip {
            defaulted.push(quote! { #ident: ::core::default::Default::default() });
            continue;
        }
        let column = opts.rename.unwrap_or_else(|| ident.to_string());
        decode.push(quote! { #ident: ::sqlx::Row::try_get(row, #column)? });
        columns.push(column);
    }

    if columns.is_empty() {
        return Err(syn::Error::new_spanned(
            name,
            "PgModel needs at least one column; every field is #[xmtp(skip)]",
        ));
    }

    // Gated the same way as the hand-written sqlx impls: `async` brings sqlx in,
    // and the async track never targets wasm.
    let gate = quote! { #[cfg(all(feature = "async", not(target_arch = "wasm32")))] };

    Ok(quote! {
        #gate
        impl crate::pg::PgModel for #name {
            const TABLE: &'static str = #table;
            const COLUMNS: &'static [&'static str] = &[#(#columns),*];
        }

        #gate
        impl<'r> ::sqlx::FromRow<'r, ::sqlx::postgres::PgRow> for #name {
            fn from_row(row: &'r ::sqlx::postgres::PgRow) -> ::sqlx::Result<Self> {
                ::core::result::Result::Ok(Self {
                    #(#decode,)*
                    #(#defaulted,)*
                })
            }
        }
    })
}
