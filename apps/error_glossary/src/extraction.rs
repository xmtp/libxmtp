use std::path::Path;

use crate::model::{ErrorType, ErrorTypeKind, ErrorVariant};

/// Parse a single Rust source file and extract all ErrorCode-deriving types.
pub fn extract_error_types(source: &str, file_path: &Path, workspace: &Path) -> Vec<ErrorType> {
    let Ok(syntax) = syn::parse_file(source) else {
        return Vec::new();
    };

    let relative = file_path
        .strip_prefix(workspace)
        .unwrap_or(file_path)
        .to_string_lossy()
        .into_owned();

    let mut result = Vec::new();
    collect_items(&syntax.items, &relative, false, &mut result);
    result
}

/// Recursively collect ErrorCode types from items, skipping test modules.
fn collect_items(
    items: &[syn::Item],
    source_file: &str,
    in_test: bool,
    out: &mut Vec<ErrorType>,
) {
    if in_test {
        return;
    }

    for item in items {
        match item {
            syn::Item::Mod(m) => {
                let is_test = is_test_module(m);
                if let Some((_, ref items)) = m.content {
                    collect_items(items, source_file, is_test, out);
                }
            }
            syn::Item::Enum(e) => {
                if !derives_error_code(&e.attrs) {
                    continue;
                }
                let container_attr = parse_error_code_attrs(&e.attrs);
                // Skip remote type impls - they document external types
                if container_attr.remote.is_some() {
                    continue;
                }
                let type_name = e.ident.to_string();
                let doc = extract_doc_comment(&e.attrs);

                let variants = e
                    .variants
                    .iter()
                    .map(|v| {
                        let vattr = parse_error_code_attrs(&v.attrs);
                        let variant_name = v.ident.to_string();
                        let default_code = format!("{}::{}", type_name, variant_name);
                        let error_code = vattr.code.unwrap_or(default_code);
                        ErrorVariant {
                            error_code,
                            doc_comment: extract_doc_comment(&v.attrs),
                            inherit: vattr.inherit,
                        }
                    })
                    .collect();

                out.push(ErrorType {
                    name: type_name,
                    kind: ErrorTypeKind::Enum,
                    source_file: source_file.to_string(),
                    doc_comment: doc,
                    internal: container_attr.internal,
                    variants,
                });
            }
            syn::Item::Struct(s) => {
                if !derives_error_code(&s.attrs) {
                    continue;
                }
                let container_attr = parse_error_code_attrs(&s.attrs);
                if container_attr.remote.is_some() {
                    continue;
                }
                let type_name = s.ident.to_string();
                let doc = extract_doc_comment(&s.attrs);
                let error_code = container_attr.code.unwrap_or_else(|| type_name.clone());

                out.push(ErrorType {
                    name: type_name,
                    kind: ErrorTypeKind::Struct,
                    source_file: source_file.to_string(),
                    doc_comment: doc.clone(),
                    internal: container_attr.internal,
                    variants: vec![ErrorVariant {
                        error_code,
                        doc_comment: doc,
                        inherit: false,
                    }],
                });
            }
            _ => {}
        }
    }
}

/// Check if a module has `#[cfg(test)]` or is named `tests`.
fn is_test_module(m: &syn::ItemMod) -> bool {
    if m.ident == "tests" || m.ident == "test" {
        return true;
    }
    m.attrs.iter().any(|attr| {
        if !attr.path().is_ident("cfg") {
            return false;
        }
        // Check if the cfg contains "test"
        let mut found_test = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("test") {
                found_test = true;
            }
            Ok(())
        });
        found_test
    })
}

/// Check if an item has `#[derive(...ErrorCode...)]`.
fn derives_error_code(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }
        let Ok(nested) = attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
        ) else {
            return false;
        };
        nested.iter().any(|path| {
            path.segments
                .last()
                .map(|s| s.ident == "ErrorCode")
                .unwrap_or(false)
        })
    })
}

/// Extract `///` doc comments from attributes.
/// Returns the joined doc text with leading/trailing whitespace trimmed per line.
fn extract_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    let doc_lines: Vec<String> = attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| {
            if let syn::Meta::NameValue(nv) = &a.meta {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
                {
                    return Some(s.value());
                }
            }
            None
        })
        .collect();

    if doc_lines.is_empty() {
        return None;
    }

    let text = doc_lines
        .iter()
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

struct ErrorCodeAttrParsed {
    code: Option<String>,
    inherit: bool,
    internal: bool,
    remote: Option<String>,
}

/// Parse `#[error_code(...)]` attributes, mirroring the proc macro logic.
fn parse_error_code_attrs(attrs: &[syn::Attribute]) -> ErrorCodeAttrParsed {
    let mut result = ErrorCodeAttrParsed {
        code: None,
        inherit: false,
        internal: false,
        remote: None,
    };

    for attr in attrs {
        if !attr.path().is_ident("error_code") {
            continue;
        }

        // Try #[error_code("CustomCode")]
        if let Ok(lit) = attr.parse_args::<syn::LitStr>() {
            result.code = Some(lit.value());
            continue;
        }

        // Try #[error_code(inherit)] or #[error_code(remote = "...")]
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("inherit") {
                result.inherit = true;
            } else if meta.path.is_ident("internal") {
                result.internal = true;
            } else if meta.path.is_ident("remote") {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                result.remote = Some(lit.value());
            }

            Ok(())
        });
    }

    result
}
