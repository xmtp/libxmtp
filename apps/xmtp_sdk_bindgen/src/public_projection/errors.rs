//! Public thrown errors (Decision 15).
//!
//! A binding error enum whose every variant carries `ErrorDetails` first
//! becomes a public error class with plain `details` and one subclass per
//! variant, so `instanceof` still works. A variant can carry more unnamed fields
//! after its details; its subclass exposes each one as a readonly property
//! named after the field's type, for example `attachmentFailure`. No binding
//! error class reaches the public API.

use std::fmt::Write as _;

use anyhow::{Result, bail};
use heck::ToLowerCamelCase as _;
use uniffi_meta::{EnumMetadata, FieldMetadata, Type, VariantMetadata};

use super::{Target, convert, public_type};

/// True for an error enum that the public layer throws as classes.
pub(super) fn is_details_error(value: &EnumMetadata) -> bool {
    value.shape.is_error() && !value.variants.is_empty() && value.variants.iter().all(|variant| {
        matches!(
            variant.fields.first(),
            Some(field) if matches!(&field.ty, Type::Record { name, .. } if name == "ErrorDetails")
        ) && variant.fields.iter().all(|field| field.name.is_empty())
    })
}

/// The fields a variant carries after its details, with their public names.
fn extra_fields(variant: &VariantMetadata) -> Result<Vec<(String, &FieldMetadata)>> {
    let extras = variant
        .fields
        .iter()
        .skip(1)
        .map(|field| match &field.ty {
            Type::Record { name, .. } | Type::Enum { name, .. } => {
                Ok((name.to_lower_camel_case(), field))
            }
            other => bail!(
                "{}: an error field after the details must be a record or enum, not {other:?}",
                variant.name
            ),
        })
        .collect::<Result<Vec<_>>>()?;
    for (index, (name, _)) in extras.iter().enumerate() {
        if name == "details" || extras[..index].iter().any(|(other, _)| other == name) {
            bail!("{}: two error fields share the name {name}", variant.name);
        }
    }
    Ok(extras)
}

pub(super) fn error_class(code: &mut String, value: &EnumMetadata, target: Target) -> Result<()> {
    let name = &value.name;
    if target == Target::Browser {
        // The browser package has one error class: the pure module defines it,
        // so `instanceof` holds for errors from the worker and from the pure
        // module alike.
        writeln!(
            code,
            "import {{ {name} }} from \"../typescript-pure/public-values.gen.js\";\nexport {{ {name} }};"
        )?;
    } else {
        class(code, value)?;
    }
    conversions(code, value)
}

fn class(code: &mut String, value: &EnumMetadata) -> Result<()> {
    let name = &value.name;
    writeln!(
        code,
        "/** A failure from the XMTP SDK. Its subclass names the failure variant. `details.code` keeps the Rust cause code. */\nexport class {name} extends Error {{\n  readonly details: ErrorDetails;\n  constructor(details: ErrorDetails) {{\n    super(details.message);\n    this.name = `{name}.${{details.code}}`;\n    this.details = details;\n  }}"
    )?;
    // Each variant has its own named subclass. The details keep the Rust
    // cause code, which can differ from the variant name. The subclasses are module-private and
    // declared after the base class, which keeps the declarations small.
    for variant in &value.variants {
        writeln!(
            code,
            "  declare static readonly {v}: typeof {name}{v};",
            v = variant.name
        )?;
    }
    code.push_str("}\n");
    for variant in &value.variants {
        let v = &variant.name;
        writeln!(code, "class {name}{v} extends {name} {{")?;
        let extras = extra_fields(variant)?;
        if !extras.is_empty() {
            for (field, metadata) in &extras {
                writeln!(code, "  readonly {field}: {};", public_type(&metadata.ty))?;
            }
            let parameters = extras
                .iter()
                .map(|(field, metadata)| format!("{field}: {}", public_type(&metadata.ty)))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                code,
                "  constructor(details: ErrorDetails, {parameters}) {{\n    super(details);"
            )?;
            for (field, _) in &extras {
                writeln!(code, "    this.{field} = {field};")?;
            }
            code.push_str("  }\n");
        }
        writeln!(
            code,
            "}}\nObject.defineProperty({name}, \"{v}\", {{ value: {name}{v} }});"
        )?;
    }
    Ok(())
}

fn conversions(code: &mut String, value: &EnumMetadata) -> Result<()> {
    let name = &value.name;
    // Lift: one subclass per binding variant.
    writeln!(
        code,
        "export function lift{name}(value: B.{name}, projection: ObjectProjection): {name} {{\n  const details = liftErrorDetails(value.inner[0], projection);\n  switch (value.tag) {{"
    )?;
    for variant in &value.variants {
        let extras = extra_fields(variant)?
            .iter()
            .enumerate()
            .map(|(index, (_, field))| {
                format!(
                    ", {}",
                    convert(&field.ty, &format!("value.inner[{}]", index + 1), false)
                )
            })
            .collect::<String>();
        writeln!(
            code,
            "    case B.{name}_Tags.{v}: return new {name}.{v}(details{extras});",
            v = variant.name
        )?;
    }
    code.push_str("  }\n}\n");
    // Lower: the binding variant of the public subclass.
    writeln!(
        code,
        "export function lower{name}(value: {name}, projection: ObjectProjection): B.{name} {{\n  const details = lowerErrorDetails(value.details, projection);"
    )?;
    for variant in &value.variants {
        let v = &variant.name;
        let extras = extra_fields(variant)?;
        if extras.is_empty() {
            writeln!(
                code,
                "  if (value.constructor === {name}.{v}) return B.{name}.{v}.new(details);"
            )?;
            continue;
        }
        let arguments = extras
            .iter()
            .map(|(field, metadata)| {
                format!(
                    ", {}",
                    convert(&metadata.ty, &format!("value.{field}"), true)
                )
            })
            .collect::<String>();
        writeln!(
            code,
            "  if (value instanceof {name}.{v} && value.constructor === {name}.{v}) return B.{name}.{v}.new(details{arguments});"
        )?;
    }
    writeln!(
        code,
        "  throw new TypeError(\"not an XMTP error subclass\");\n}}"
    )?;
    // Detect a binding error without a cast: its tag is one of the enum tags.
    writeln!(
        code,
        "const {lower}Tags: ReadonlySet<unknown> = new Set(Object.values(B.{name}_Tags));\nfunction isBinding{name}(value: unknown): value is B.{name} {{\n  return value instanceof Error && \"tag\" in value && \"inner\" in value && {lower}Tags.has(value.tag);\n}}",
        lower = heck::ToLowerCamelCase::to_lower_camel_case(name.as_str())
    )?;
    Ok(())
}

/// Browser only: a package worker or transport failure (`BridgeError`) has no
/// binding error. It becomes the public error of its code when that code is a
/// variant, and `Unknown` with the same plain details otherwise.
pub(super) fn bridge_error(code: &mut String, value: &EnumMetadata) -> Result<()> {
    let name = &value.name;
    writeln!(
        code,
        "const bindingCategories: ReadonlySet<unknown> = new Set(Object.values(B.ErrorCategory));\nfunction isBindingCategory(value: unknown): value is B.ErrorCategory {{\n  return typeof value === \"number\" && bindingCategories.has(value);\n}}\nfunction liftBridgeError(error: BridgeError, projection: ObjectProjection): {name} {{\n  const details: ErrorDetails = {{\n    code: error.code,\n    category: isBindingCategory(error.category) ? liftErrorCategory(error.category, projection) : \"unknown\",\n    retryable: error.retryable,\n    message: error.message,\n  }};\n  switch (error.code) {{"
    )?;
    // A variant with fields after its details has no public form without
    // them, so its code takes the fallback.
    for variant in value.variants.iter().filter(|v| v.fields.len() == 1) {
        writeln!(
            code,
            "    case \"{v}\": return new {name}.{v}(details);",
            v = variant.name
        )?;
    }
    let fallback = if value
        .variants
        .iter()
        .any(|variant| variant.name == "Unknown")
    {
        format!("new {name}.Unknown({{ ...details, code: \"Unknown\" }})")
    } else {
        format!("new {name}(details)")
    };
    writeln!(code, "    default: return {fallback};\n  }}\n}}")?;
    Ok(())
}

/// Convert a thrown binding error, and in the browser a package worker or
/// transport failure, to its public error. Other values pass through
/// unchanged.
pub(super) fn public_error(target: Target) -> &'static str {
    match target {
        Target::Node | Target::Pure => {
            r#"
/** The public form of a thrown value. A binding error becomes an `XmtpError`. */
export function publicError(error: unknown): unknown {
  return isBindingXmtpError(error) ? liftXmtpError(error, currentProjection()) : error;
}
"#
        }
        Target::Browser => {
            r#"
/**
 * The public form of a thrown value. A binding error, or a package worker or
 * transport failure, becomes an `XmtpError`.
 */
export function publicError(error: unknown): unknown {
  if (isBindingXmtpError(error)) return liftXmtpError(error, currentProjection());
  if (error instanceof BridgeError) return liftBridgeError(error, currentProjection());
  return error;
}
"#
        }
    }
}
