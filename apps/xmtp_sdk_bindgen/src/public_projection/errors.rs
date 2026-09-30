//! Public thrown errors (Decision 15).
//!
//! A binding error enum whose every variant carries one `ErrorDetails` becomes
//! a public error class with plain `details` and one subclass per code, so
//! `instanceof` still works. No binding error class reaches the public API.

use std::fmt::Write as _;

use anyhow::Result;
use uniffi_meta::{EnumMetadata, Type};

/// True for an error enum that the public layer throws as classes.
pub(super) fn is_details_error(value: &EnumMetadata) -> bool {
    value.shape.is_error()
        && !value.variants.is_empty()
        && value.variants.iter().all(|variant| {
            matches!(
                variant.fields.as_slice(),
                [field] if matches!(&field.ty, Type::Record { name, .. } if name == "ErrorDetails")
            )
        })
}

pub(super) fn error_class(code: &mut String, value: &EnumMetadata) -> Result<()> {
    let name = &value.name;
    writeln!(
        code,
        "/** A failure from the XMTP SDK. Its subclass names the code in `details.code`. */\nexport class {name} extends Error {{\n  readonly details: ErrorDetails;\n  constructor(details: ErrorDetails) {{\n    super(details.message);\n    this.name = `{name}.${{details.code}}`;\n    this.details = details;\n  }}"
    )?;
    // Each code has its own named subclass, so `instanceof` narrows to that
    // code and keeps the other codes. The subclasses are module-private and
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
        writeln!(
            code,
            "class {name}{v} extends {name} {{\n  declare readonly details: ErrorDetails & {{ readonly code: \"{v}\" }};\n}}\nObject.defineProperty({name}, \"{v}\", {{ value: {name}{v} }});",
            v = variant.name
        )?;
    }
    // Lift: one subclass per binding variant.
    writeln!(
        code,
        "export function lift{name}(value: B.{name}, projection: ObjectProjection): {name} {{\n  const details = liftErrorDetails(value.inner[0], projection);\n  switch (value.tag) {{"
    )?;
    for variant in &value.variants {
        writeln!(
            code,
            "    case B.{name}_Tags.{v}: return new {name}.{v}(details);",
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
        writeln!(
            code,
            "  if (value.constructor === {name}.{v}) return B.{name}.{v}.new(details);",
            v = variant.name
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

/// Convert a thrown binding error to its public error. Other values pass
/// through unchanged.
pub(super) const PUBLIC_ERROR: &str = r#"
/** The public form of a thrown value. A binding error becomes an `XmtpError`. */
export function publicError(error: unknown): unknown {
  return isBindingXmtpError(error) ? liftXmtpError(error, currentProjection()) : error;
}
"#;
