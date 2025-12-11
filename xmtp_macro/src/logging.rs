use std::collections::HashSet;
use syn::{Attribute, Expr, Meta, Variant};

/// Value format patterns for tracing fields: `field = expr`, `field = %expr`, `field = ?expr`, `field` (shorthand)
const VALUE_PATTERNS: &[&str] = &["= $val:expr", "= %$val:expr", "= ?$val:expr", ""];

/// Generate macro arms that enforce required context fields while allowing extra fields.
///
/// Strategy: Validate that required fields exist, then pass ALL original tokens directly
/// to tracing::info! to preserve hygiene. We do this by:
/// 1. Saving the original input tokens
/// 2. Scanning a copy of input tokens to check for required field names
/// 3. If all required fields found, emit tracing::info! with original tokens
/// 4. If missing fields, emit compile_error!
pub(crate) fn generate_field_arms(
    full_path: &str,
    fields: &[String],
    doc_comment: &str,
    _has_inbox_id: bool,
) -> Vec<String> {
    // Deduplicate fields while preserving order
    let mut seen = HashSet::new();
    let unique_fields: Vec<_> = fields.iter().filter(|f| seen.insert(*f)).collect();

    if unique_fields.is_empty() {
        return vec![format!(
            r#"({} $(, $($fields:tt)*)?) => {{
        ::tracing::info!($($($fields)*)? {})
    }};"#,
            full_path,
            quote_string(doc_comment),
        )];
    }

    let n = unique_fields.len();
    let fields_list = unique_fields
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    // Helper to generate state patterns like "$s0:tt $s1:tt ..."
    let state_pattern = |start: usize, count: usize| {
        (start..start + count)
            .map(|j| format!("$s{}:tt", j))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let state_output = |start: usize, count: usize| {
        (start..start + count)
            .map(|j| format!("$s{}", j))
            .collect::<Vec<_>>()
            .join(" ")
    };

    let initial_state = unique_fields
        .iter()
        .map(|f| format!("[{} missing]", f))
        .collect::<Vec<_>>()
        .join(" ");
    let success_state = unique_fields
        .iter()
        .map(|f| format!("[{} found]", f))
        .collect::<Vec<_>>()
        .join(" ");

    let mut arms = Vec::with_capacity(n * 8 + 7);

    // Entry point: save original tokens and start validation
    arms.push(format!(
        r#"({} $(, $($input:tt)*)?) => {{
        log_event!(@validate {} {} @orig[ $($($input)*)? ] @cur[ $($($input)*)? ])
    }};"#,
        full_path, full_path, initial_state
    ));

    // Per-field validation arms: handle both "missing" and "found" states
    for (i, field) in unique_fields.iter().enumerate() {
        let before = state_pattern(0, i);
        let after = state_pattern(i + 1, n - i - 1);
        let before_out = state_output(0, i);
        let after_out = state_output(i + 1, n - i - 1);

        for state in ["missing", "found"] {
            for pattern in VALUE_PATTERNS {
                let field_pattern = if pattern.is_empty() {
                    field.to_string()
                } else {
                    format!("{} {}", field, pattern)
                };

                arms.push(format!(
                    r#"(@validate {} {} [{} {}] {} @orig[ $($orig:tt)* ] @cur[ {} $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} [{} found] {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
                    full_path, before, field, state, after, field_pattern,
                    full_path, before_out, field, after_out
                ));
            }
        }
    }

    // Non-required field handlers: skip any field not in the required list
    let all_state = state_pattern(0, n);
    let all_out = state_output(0, n);

    for pattern in VALUE_PATTERNS {
        let field_pattern = if pattern.is_empty() {
            "$field:tt".to_string()
        } else {
            format!("$field:tt {}", pattern)
        };

        arms.push(format!(
            r#"(@validate {} {} @orig[ $($orig:tt)* ] @cur[ {} $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
            full_path, all_state, field_pattern, full_path, all_out
        ));
    }

    // Success terminal: all required fields found
    arms.push(format!(
        r#"(@validate {} {} @orig[ $($orig:tt)* ] @cur[ ]) => {{
        ::tracing::info!($($orig)* {})
    }};"#,
        full_path,
        success_state,
        quote_string(doc_comment)
    ));

    // Error terminals: missing required fields
    for (i, field) in unique_fields.iter().enumerate() {
        let before = state_pattern(0, i);
        let after = state_pattern(i + 1, n - i - 1);

        arms.push(format!(
            r#"(@validate {} {} [{} missing] {} @orig[ $($orig:tt)* ] @cur[ ]) => {{
        ::core::compile_error!(concat!("{} requires context fields: {}", ". Missing field: {}."))
    }};"#,
            full_path, before, field, after, full_path, fields_list, field
        ));
    }

    arms
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

pub(crate) fn quote_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
