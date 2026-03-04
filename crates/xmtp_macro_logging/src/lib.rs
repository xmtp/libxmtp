extern crate proc_macro;

mod logging;

use proc_macro2::*;
use quote::quote;
use syn::{Data, DeriveInput, parse_macro_input};

use crate::logging::{LogEventInput, get_context_fields, get_doc_comment, get_icon};

#[proc_macro_attribute]
pub fn build_logging_metadata(
    _attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    let enum_name = &input.ident;
    let visibility = &input.vis;
    let attrs = &input.attrs;

    let Data::Enum(data_enum) = &input.data else {
        return syn::Error::new_spanned(&input, "log_event_macro can only be used on enums")
            .to_compile_error()
            .into();
    };

    let mut display_arms = Vec::new();
    let mut metadata_entries = Vec::new();
    let mut cleaned_variants = Vec::new();
    let mut metadata_match_arms = Vec::new();

    for variant in &data_enum.variants {
        let variant_name = &variant.ident;
        let variant_name_str = variant_name.to_string();
        let doc_comment = match get_doc_comment(variant) {
            Ok(dc) => dc,
            Err(err) => return err.to_compile_error().into(),
        };
        let icon = get_icon(&variant.attrs).unwrap_or_default();
        let context_fields = get_context_fields(&variant.attrs);

        // Filter out #[context(...)] attributes for the output enum
        let filtered_attrs: Vec<_> = variant
            .attrs
            .iter()
            .filter(|a| !a.path().is_ident("context"))
            .collect();

        // Rebuild variant without context attribute
        let variant_fields = &variant.fields;
        let variant_discriminant = variant
            .discriminant
            .as_ref()
            .map(|(eq, expr)| quote! { #eq #expr });

        cleaned_variants.push(quote! {
            #(#filtered_attrs)*
            #variant_name #variant_fields #variant_discriminant
        });

        // Display impl arm
        display_arms.push(quote! {
            #enum_name::#variant_name => write!(f, #doc_comment),
        });

        // Metadata entry for the const array
        let context_fields_tokens: Vec<_> = context_fields.iter().map(|f| quote! { #f }).collect();
        metadata_entries.push(quote! {
            crate::EventMetadata {
                name: #variant_name_str,
                event: #enum_name::#variant_name,
                doc: #doc_comment,
                context_fields: &[#(#context_fields_tokens),*],
                icon: #icon,
            }
        });

        // Match arm for the metadata() method
        metadata_match_arms.push(quote! {
            #enum_name::#variant_name => &Self::METADATA[#enum_name::#variant_name as usize],
        });
    }

    let variant_count = cleaned_variants.len();

    let expanded = quote! {
        #(#attrs)*
        #[repr(usize)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #visibility enum #enum_name {
            #(#cleaned_variants),*
        }

        impl #enum_name {
            /// Metadata for all variants of this enum, indexed by variant discriminant.
            pub const METADATA: [crate::EventMetadata; #variant_count] = [
                #(#metadata_entries),*
            ];

            /// Returns the metadata for this event variant.
            pub const fn metadata(&self) -> &'static crate::EventMetadata {
                match self {
                    #(#metadata_match_arms)*
                }
            }
        }

        impl ::core::fmt::Display for #enum_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#display_arms)*
                }
            }
        }
    };

    expanded.into()
}

#[proc_macro]
pub fn log_event(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as LogEventInput);
    let event = &input.event;
    let installation_id = &input.installation_id;

    let provided_names: Vec<String> = input.fields.iter().map(|f| f.name.to_string()).collect();
    let tracing_fields: Vec<TokenStream> =
        input.fields.iter().map(|f| f.to_tracing_tokens()).collect();

    // Generate match arms for building context string (non-structured logging only)
    let context_match_arms: Vec<TokenStream> = input
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let name_str = &provided_names[i];
            let value = f.value_tokens();
            if matches!(f.sigil, Some('#')) {
                // # sigil (short_hex): always quote the value so hex strings
                // like "1713e608" aren't misinterpreted as scientific notation
                quote! {
                    #name_str => Some(format!("{}: \"{}\"", #name_str, #value))
                }
            } else if matches!(f.sigil, Some('$')) {
                // $ sigil (json): value is already a JSON string from serde_json::to_string
                quote! {
                    #name_str => Some(format!("{}: {}", #name_str, #value))
                }
            } else if matches!(f.sigil, Some('%')) {
                quote! {
                    #name_str => Some(format!("{}: {}", #name_str, #value))
                }
            } else {
                quote! {
                    #name_str => Some(format!("{}: {:?}", #name_str, #value))
                }
            }
        })
        .collect();

    let provided_names_tokens = provided_names.into_iter().map(|n| quote! { #n });

    // Generate the appropriate tracing level
    let level = match input.level {
        logging::LogLevel::Info => quote! { ::tracing::Level::INFO },
        logging::LogLevel::Warn => quote! { ::tracing::Level::WARN },
        logging::LogLevel::Error => quote! { ::tracing::Level::ERROR },
    };

    let tracing_call = quote! {
        ::tracing::event!(
            #level,
            #(#tracing_fields,)*
            "{}",
            __message
        );
    };

    quote! {
        {
            const PROVIDED: &[&str] = &[#(#provided_names_tokens),*];

            // Compile-time validation: ensure all required context fields are provided
            const _: () = #event.metadata().validate_fields(PROVIDED);

            let __meta = #event.metadata();

            // Bind installation_id to a variable to extend its lifetime
            let __installation_id = #installation_id;
            let __inst = {
                use xmtp_proto::ShortHex;
                __installation_id.short_hex()
            };

            let __time_ns = xmtp_common::time::now_ns();

            // Build message with context for non-structured logging
            let __message = if ::xmtp_common::is_structured_logging() {
                // Structured logging: include installation_id and timestamp in message
                format!("➣ {} {{time: {__time_ns}, inst: \"{__inst}\"}}", __meta.doc)
            } else {
                // Non-structured logging: embed context in message for readability
                let mut __context_parts: ::std::vec::Vec<String> = __meta.context_fields
                    .iter()
                    .filter_map(|&field_name| {
                        match field_name {
                            #(#context_match_arms,)*
                            _ => None,
                        }
                    })
                    .collect();

                __context_parts.push(format!("time: {__time_ns}"));
                __context_parts.push(format!("inst: \"{__inst}\""));
                let __context_str = __context_parts.join(", ").replace('\n', " ");

                format!("➣ {} {{{__context_str}}}", __meta.doc)
            };

            #tracing_call
        }
    }
    .into()
}
