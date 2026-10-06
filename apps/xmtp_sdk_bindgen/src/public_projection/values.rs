use std::fmt::Write as _;

use anyhow::{Result, bail};
use uniffi_meta::{EnumMetadata, RecordMetadata, Type};

use super::{
    camel, convert,
    events::{EVENT_ENUM, Events},
    policy::cursor_type,
    public_type,
};

pub(super) fn record(code: &mut String, record: &RecordMetadata, events: &Events) -> Result<()> {
    let name = &record.name;
    if record.fields.is_empty() {
        writeln!(code, "export type {name} = Record<string, never>;\n")?;
    } else {
        writeln!(code, "export type {name} = {{")?;
    }
    for field in &record.fields {
        // The binding factory fills Rust defaults. EventFilter has a public
        // false default that lowering supplies before binding conversion.
        let optional = if matches!(field.ty, Type::Optional { .. })
            || field.default.is_some()
            || (name == "EventFilter" && field.name == "references_own_messages")
        {
            "?"
        } else {
            ""
        };
        writeln!(
            code,
            "readonly {}{optional}: {};",
            public_record_field(name, &field.name, events),
            cursor_type(name, &camel(&field.name), public_type(&field.ty))
        )?;
    }
    if !record.fields.is_empty() {
        code.push_str("};\n");
    }
    for lower in [false, true] {
        let (direction, source, target) = if lower {
            ("lower", name.to_owned(), format!("B.{name}"))
        } else {
            ("lift", format!("B.{name}"), name.to_owned())
        };
        let defaults = lower && record.fields.iter().any(|field| field.default.is_some());
        let fields = record
            .fields
            .iter()
            .map(|field| {
                let binding_name = camel(&field.name);
                let public_name = public_record_field(name, &field.name, events);
                if defaults && field.default.is_some() {
                    // An explicit undefined would replace the factory default,
                    // so a left-out field stays out.
                    let ty = match &field.ty {
                        Type::Optional { inner_type } => inner_type,
                        ty => ty,
                    };
                    let converted = convert(ty, &format!("value.{public_name}"), lower);
                    (
                        true,
                        format!(
                            "value.{public_name} === undefined ? {{}} : {{ {binding_name}: {converted} }}"
                        ),
                    )
                } else {
                    let source_name = if lower { &public_name } else { &binding_name };
                    let output_name = if lower { &binding_name } else { &public_name };
                    let source = if lower
                        && name == "EventFilter"
                        && field.name == "references_own_messages"
                    {
                        format!("(value.{source_name} ?? false)")
                    } else {
                        format!("value.{source_name}")
                    };
                    let converted = convert(&field.ty, &source, lower);
                    (false, format!("{output_name}: {converted}"))
                }
            })
            .collect::<Vec<_>>();
        let body = match fields.as_slice() {
            [(true, only)] => only.clone(),
            fields => format!(
                "{{ {} }}",
                fields
                    .iter()
                    .map(|(spread, field)| if *spread {
                        format!("...({field})")
                    } else {
                        field.clone()
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        let body = if defaults {
            format!("B.{name}.create({body})")
        } else {
            body
        };
        let guard = if lower && name == "Credential" {
            super::policy::CREDENTIAL_GUARD
        } else {
            ""
        };
        let unused_value = if record.fields.is_empty() {
            "void value; "
        } else {
            ""
        };
        writeln!(
            code,
            "export function {direction}{name}(value: {source}, projection: ObjectProjection): {target} {{ void projection; {unused_value}{guard} return {body}; }}"
        )?;
    }
    Ok(())
}

fn public_record_field(record: &str, field: &str, events: &Events) -> String {
    if events.keeps_rust_fields(record) {
        field.to_owned()
    } else {
        camel(field)
    }
}

pub(super) fn enumeration(code: &mut String, value: &EnumMetadata, events: &Events) -> Result<()> {
    let name = &value.name;
    if name == EVENT_ENUM
        && value
            .variants
            .iter()
            .any(|variant| variant.fields.len() != 1 || variant.fields[0].name.is_empty())
    {
        bail!("each {EVENT_ENUM} kind must have one named payload");
    }
    let flat = !value.shape.is_error() && value.variants.iter().all(|v| v.fields.is_empty());
    writeln!(code, "export type {name} =")?;
    for variant in &value.variants {
        let kind = events.variant_kind(name, variant)?;
        if flat {
            writeln!(code, "| '{kind}'")?;
            continue;
        }
        writeln!(code, "| {{ readonly kind: '{kind}';")?;
        for (i, field) in variant.fields.iter().enumerate() {
            let field_name = if field.name.is_empty() {
                if variant.fields.len() == 1 {
                    "value".into()
                } else {
                    format!("value{i}")
                }
            } else {
                if name == EVENT_ENUM {
                    field.name.clone()
                } else {
                    camel(&field.name)
                }
            };
            let optional = if matches!(field.ty, Type::Optional { .. }) {
                "?"
            } else {
                ""
            };
            writeln!(
                code,
                "readonly {field_name}{optional}: {};",
                public_type(&field.ty)
            )?;
        }
        code.push_str(super::policy::extra_variant_fields(name, &variant.name));
        code.push_str("}\n");
    }
    code.push_str(";\n");
    for lower in [false, true] {
        let (direction, source, target) = if lower {
            ("lower", name.to_owned(), format!("B.{name}"))
        } else {
            ("lift", format!("B.{name}"), name.to_owned())
        };
        let discriminant = if flat {
            "value"
        } else if lower {
            "value.kind"
        } else {
            "value.tag"
        };
        let single = value.variants.len() == 1;
        writeln!(
            code,
            "export function {direction}{name}(value: {source}, projection: ObjectProjection): {target} {{ void projection;"
        )?;
        if lower && name == "AttachmentSource" {
            code.push_str(super::policy::ATTACHMENT_SOURCE_GUARD);
        }
        if !single {
            writeln!(code, "switch ({discriminant}) {{")?;
        }
        for variant in &value.variants {
            let kind = events.variant_kind(name, variant)?;
            let raw_variant = format!("B.{name}.{}", variant.name);
            let case = if lower {
                format!("'{kind}'")
            } else if flat {
                raw_variant.clone()
            } else {
                format!("B.{name}_Tags.{}", variant.name)
            };
            let prefix = if single {
                format!("checkedVariant({discriminant}, {case});")
            } else {
                format!("case {case}:")
            };
            if flat {
                writeln!(
                    code,
                    "{prefix} return {};",
                    if lower {
                        raw_variant
                    } else {
                        format!("'{kind}'")
                    }
                )?;
                continue;
            }
            let named = variant.fields.first().is_some_and(|f| !f.name.is_empty());
            let fields = variant
                .fields
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let binding_name = if field.name.is_empty() {
                        if variant.fields.len() == 1 {
                            "value".into()
                        } else {
                            format!("value{i}")
                        }
                    } else {
                        camel(&field.name)
                    };
                    let public_name = if name == EVENT_ENUM && !field.name.is_empty() {
                        field.name.clone()
                    } else {
                        binding_name.clone()
                    };
                    let raw = if lower {
                        format!("value.{public_name}")
                    } else if named {
                        format!("value.inner.{binding_name}")
                    } else {
                        format!("value.inner[{i}]")
                    };
                    let converted = convert(&field.ty, &raw, lower);
                    if !lower || named {
                        let output_name = if lower { binding_name } else { public_name };
                        format!("{output_name}: {converted}")
                    } else {
                        converted
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let result = if lower {
                let args = if named {
                    format!("{{{fields}}}")
                } else {
                    fields
                };
                format!("{raw_variant}.new({args})")
            } else {
                format!("{{kind: '{kind}', {fields}}}")
            };
            writeln!(code, "{prefix} return {result};")?;
        }
        if !single {
            code.push_str("default: throw new TypeError('invalid public enum'); }\n");
        }
        code.push_str("}\n");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use uniffi_meta::{FieldMetadata, Metadata};

    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn event_filter_omission_has_a_public_false_default() -> anyhow::Result<()> {
        let mut record = RecordMetadata {
            module_path: "test".into(),
            name: "EventFilter".into(),
            orig_name: None,
            remote: false,
            fields: vec![FieldMetadata {
                name: "references_own_messages".into(),
                orig_name: None,
                ty: Type::Boolean,
                default: None,
                docstring: None,
            }],
            docstring: None,
        };
        let filter = Metadata::Record(record.clone());
        let events = Events::new(&[&filter])?;
        let mut code = String::new();
        super::record(&mut code, &record, &events)?;
        assert!(code.contains("readonly references_own_messages?: boolean;"));
        assert!(code.contains("referencesOwnMessages: (value.references_own_messages ?? false)"));
        // A public default must not make unrelated required booleans optional,
        // and a record outside the event set uses camelCase fields.
        record.name = "OtherRecord".into();
        code.clear();
        super::record(&mut code, &record, &events)?;
        assert!(code.contains("readonly referencesOwnMessages: boolean;"));
        assert!(!code.contains("?? false"));
        Ok(())
    }
}
