use std::collections::HashSet;
use syn::{Attribute, Expr, Meta};

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
    // Deduplicate fields
    let mut seen = HashSet::new();
    let unique_fields: Vec<&String> = fields.iter().filter(|f| seen.insert(*f)).collect();

    if unique_fields.is_empty() {
        // No required fields - just pass through
        let arm = format!(
            r#"({} $(, $($fields:tt)*)?) => {{
        ::tracing::info!($($($fields)*)? {})
    }};"#,
            full_path,
            quote_string(doc_comment),
        );
        return vec![arm];
    }

    let fields_list = unique_fields
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let mut arms = Vec::new();

    // Entry point: save original input and start validation
    // State is represented as: [field1 missing] [field2 missing] ...
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

    // Entry point: save original tokens in @orig, start validation with @cur
    arms.push(format!(
        r#"({} $(, $($input:tt)*)?) => {{
        log_event!(@validate {} {} @orig[ $($($input)*)? ] @cur[ $($($input)*)? ])
    }};"#,
        full_path, full_path, initial_state
    ));

    // For each required field, generate validation arms
    for (i, field) in unique_fields.iter().enumerate() {
        let before_pattern: String = (0..i).map(|j| format!("$s{}:tt", j)).collect::<Vec<_>>().join(" ");
        let after_pattern: String = (i + 1..unique_fields.len())
            .map(|j| format!("$s{}:tt", j))
            .collect::<Vec<_>>()
            .join(" ");

        let before_output: String = (0..i).map(|j| format!("$s{}", j)).collect::<Vec<_>>().join(" ");
        let after_output: String = (i + 1..unique_fields.len())
            .map(|j| format!("$s{}", j))
            .collect::<Vec<_>>()
            .join(" ");

        // field = expr - mark as found, skip this field in @cur
        arms.push(format!(
            r#"(@validate {} {} [{} missing] {} @orig[ $($orig:tt)* ] @cur[ {} = $val:expr $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} [{} found] {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
            full_path, before_pattern, field, after_pattern, field,
            full_path, before_output, field, after_output
        ));

        // field = %expr
        arms.push(format!(
            r#"(@validate {} {} [{} missing] {} @orig[ $($orig:tt)* ] @cur[ {} = %$val:expr $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} [{} found] {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
            full_path, before_pattern, field, after_pattern, field,
            full_path, before_output, field, after_output
        ));

        // field = ?expr
        arms.push(format!(
            r#"(@validate {} {} [{} missing] {} @orig[ $($orig:tt)* ] @cur[ {} = ?$val:expr $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} [{} found] {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
            full_path, before_pattern, field, after_pattern, field,
            full_path, before_output, field, after_output
        ));

        // field (shorthand) - mark as found
        arms.push(format!(
            r#"(@validate {} {} [{} missing] {} @orig[ $($orig:tt)* ] @cur[ {} $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} [{} found] {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
            full_path, before_pattern, field, after_pattern, field,
            full_path, before_output, field, after_output
        ));

        // When field is already found, still need to skip it in @cur
        arms.push(format!(
            r#"(@validate {} {} [{} found] {} @orig[ $($orig:tt)* ] @cur[ {} = $val:expr $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} [{} found] {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
            full_path, before_pattern, field, after_pattern, field,
            full_path, before_output, field, after_output
        ));

        arms.push(format!(
            r#"(@validate {} {} [{} found] {} @orig[ $($orig:tt)* ] @cur[ {} = %$val:expr $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} [{} found] {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
            full_path, before_pattern, field, after_pattern, field,
            full_path, before_output, field, after_output
        ));

        arms.push(format!(
            r#"(@validate {} {} [{} found] {} @orig[ $($orig:tt)* ] @cur[ {} = ?$val:expr $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} [{} found] {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
            full_path, before_pattern, field, after_pattern, field,
            full_path, before_output, field, after_output
        ));

        arms.push(format!(
            r#"(@validate {} {} [{} found] {} @orig[ $($orig:tt)* ] @cur[ {} $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} [{} found] {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
            full_path, before_pattern, field, after_pattern, field,
            full_path, before_output, field, after_output
        ));
    }

    // Handle non-required fields - just skip them in @cur
    let all_state_pattern: String = (0..unique_fields.len())
        .map(|j| format!("$s{}:tt", j))
        .collect::<Vec<_>>()
        .join(" ");
    let all_state_output: String = (0..unique_fields.len())
        .map(|j| format!("$s{}", j))
        .collect::<Vec<_>>()
        .join(" ");

    // Non-required field = expr
    arms.push(format!(
        r#"(@validate {} {} @orig[ $($orig:tt)* ] @cur[ $field:tt = $val:expr $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
        full_path, all_state_pattern,
        full_path, all_state_output
    ));

    // Non-required field = %expr
    arms.push(format!(
        r#"(@validate {} {} @orig[ $($orig:tt)* ] @cur[ $field:tt = %$val:expr $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
        full_path, all_state_pattern,
        full_path, all_state_output
    ));

    // Non-required field = ?expr
    arms.push(format!(
        r#"(@validate {} {} @orig[ $($orig:tt)* ] @cur[ $field:tt = ?$val:expr $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
        full_path, all_state_pattern,
        full_path, all_state_output
    ));

    // Non-required field (shorthand)
    arms.push(format!(
        r#"(@validate {} {} @orig[ $($orig:tt)* ] @cur[ $field:tt $(, $($rest:tt)*)? ]) => {{
        log_event!(@validate {} {} @orig[ $($orig)* ] @cur[ $($($rest)*)? ])
    }};"#,
        full_path, all_state_pattern,
        full_path, all_state_output
    ));

    // Terminal: all required fields found, @cur exhausted - emit with ORIGINAL tokens
    arms.push(format!(
        r#"(@validate {} {} @orig[ $($orig:tt)* ] @cur[ ]) => {{
        ::tracing::info!($($orig)* {})
    }};"#,
        full_path, success_state, quote_string(doc_comment)
    ));

    // Terminal: @cur exhausted but some required fields still missing -> error
    for (i, field) in unique_fields.iter().enumerate() {
        let before_pattern: String = (0..i)
            .map(|j| format!("$s{}:tt", j))
            .collect::<Vec<_>>()
            .join(" ");
        let after_pattern: String = (i + 1..unique_fields.len())
            .map(|j| format!("$s{}:tt", j))
            .collect::<Vec<_>>()
            .join(" ");

        arms.push(format!(
            r#"(@validate {} {} [{} missing] {} @orig[ $($orig:tt)* ] @cur[ ]) => {{
        ::core::compile_error!(concat!("{} requires context fields: {}", ". Missing field: {}."))
    }};"#,
            full_path, before_pattern, field, after_pattern, full_path, fields_list, field
        ));
    }

    arms
}

pub(crate) fn get_doc_comment(attrs: &[Attribute]) -> String {
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Meta::NameValue(nv) = &attr.meta {
                if let Expr::Lit(expr_lit) = &nv.value {
                    if let syn::Lit::Str(s) = &expr_lit.lit {
                        return s.value().trim().to_string();
                    }
                }
            }
        }
    }
    String::new()
}

pub(crate) fn get_context_fields(attrs: &[Attribute]) -> Vec<String> {
    for attr in attrs {
        if attr.path().is_ident("context") {
            let mut fields = Vec::new();
            let _ = attr.parse_nested_meta(|meta| {
                if let Some(ident) = meta.path.get_ident() {
                    fields.push(ident.to_string());
                }
                Ok(())
            });
            return fields;
        }
    }
    Vec::new()
}

pub(crate) fn quote_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
