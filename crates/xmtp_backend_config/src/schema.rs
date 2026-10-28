//! Scalar constraints for the public configuration schema.

use schemars::{Schema, SchemaGenerator, json_schema};

/// Preserve exact integer bounds instead of converting large limits to floating point.
pub(super) fn positive_integer<const MAX: u64>(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "integer", "minimum": 1, "maximum": MAX})
}

/// Accept a literal value or an environment reference whose value is checked at startup.
pub(super) fn with_environment(literal: Schema) -> Schema {
    json_schema!({"anyOf": [literal, {"type": "string", "pattern": "^env:[^=\\x00]+$(?![\\s\\S])"}]})
}

/// Allow the shared level names or a level supplied by an environment reference.
pub(super) fn log_level(generator: &mut SchemaGenerator) -> Schema {
    with_environment(generator.subschema_for::<super::LogLevel>())
}

/// Constrain database URLs without resolving environment values in the schema.
pub(super) fn postgres_url(_: &mut SchemaGenerator) -> Schema {
    with_environment(json_schema!({
        "type": "string", "format": "uri", "pattern": "^postgres(ql)?://[^\\s]+$"
    }))
}

/// Add the absent-replica representation to the same database URL constraint.
pub(super) fn optional_postgres_url(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({"anyOf": [postgres_url(generator), {"type": "null"}]})
}

/// Match the chain parser: an unsigned 64-bit chain id in canonical decimal,
/// with no sign and no leading zero.
pub(super) fn chains(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "object",
        "propertyNames": {"pattern": format!("^eip155:(?!0[0-9]){}$(?![\\s\\S])", decimal_range(u64::MAX))},
        "additionalProperties": with_environment(json_schema!({
            "type": "string", "format": "uri", "pattern": "^https?://[^\\s/?#]+[^\\s]*$"
        }))
    })
}

/// Match decimal integers from zero through `maximum`.
/// Each branch fixes a prefix and allows one smaller digit followed by any digits.
fn decimal_range(maximum: u64) -> String {
    let digits = maximum.to_string();
    let mut alternatives = vec![digits.clone()];
    if digits.len() > 1 {
        alternatives.push(format!("[0-9]{{1,{}}}", digits.len() - 1));
    }
    for (index, digit) in digits.bytes().enumerate() {
        if digit > b'0' {
            alternatives.push(format!(
                "{}[0-{}][0-9]{{{}}}",
                &digits[..index],
                char::from(digit - 1),
                digits.len() - index - 1
            ));
        }
    }
    format!("({})", alternatives.join("|"))
}

/// An IPv4 address in dotted decimal with no leading zero, as the standard
/// library parses it.
fn ipv4() -> String {
    let octet = "(0|[1-9][0-9]?|1[0-9]{2}|2[0-4][0-9]|25[0-5])";
    [octet; 4].join("\\.")
}

/// An IPv6 address as the standard library parses it, without brackets or a
/// zone index. Compression omits at least one group. An embedded IPv4
/// address uses two groups.
fn ipv6() -> String {
    let ipv4 = ipv4();
    let hex = "[0-9a-fA-F]{1,4}";
    let mut ipv6 = vec![[hex; 8].join(":"), format!("{}:{ipv4}", [hex; 6].join(":"))];
    for embedded in [false, true] {
        let slots = if embedded { 6 } else { 8 };
        for left in 0..slots {
            for right in 0..slots - left {
                let prefix = vec![hex; left].join(":");
                let mut suffix = vec![hex; right].join(":");
                if embedded {
                    if right > 0 {
                        suffix.push(':');
                    }
                    suffix.push_str(&ipv4);
                }
                ipv6.push(format!("{prefix}::{suffix}"));
            }
        }
    }
    ipv6.join("|")
}

/// Encode the standard library socket grammar with portable JSON Schema patterns.
pub(super) fn socket_address(_: &mut SchemaGenerator) -> Schema {
    let scope = decimal_range(u32::MAX as u64);
    let port = decimal_range(u16::MAX as u64);
    with_environment(json_schema!({
        "type": "string",
        "pattern": format!("^({}|\\[({})(%0*{scope})?\\]):0*{port}$(?![\\s\\S])", ipv4(), ipv6())
    }))
}

/// The `ip address` standard check.
pub(super) fn ip_address(_: &mut SchemaGenerator) -> Schema {
    with_environment(json_schema!({
        "type": "string",
        "pattern": format!("^({}|{})$(?![\\s\\S])", ipv4(), ipv6())
    }))
}

/// The `ip network` standard check, except that a bit set after the prefix
/// is refused only at startup.
pub(super) fn ip_network(_: &mut SchemaGenerator) -> Schema {
    with_environment(json_schema!({
        "type": "string",
        "pattern": format!(
            "^(({})/0*{}|({})/0*{})$(?![\\s\\S])",
            ipv4(),
            decimal_range(32),
            ipv6(),
            decimal_range(128)
        )
    }))
}

#[cfg(test)]
mod tests;

/// Allow stdout encoding through the same environment mechanism as log levels.
pub(super) fn log_format(generator: &mut SchemaGenerator) -> Schema {
    with_environment(generator.subschema_for::<super::LogFormat>())
}

pub(super) fn metrics_address(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({"anyOf": [socket_address(generator), {"const": ""}]})
}

pub(super) fn optional_http_url(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"anyOf": [with_environment(json_schema!({"type": "string", "format": "uri", "pattern": "^https?://[^\\s/?#]+[^\\s]*$"})), {"type": "null"}]})
}

pub(super) fn resource_attributes(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "object", "propertyNames": {"not": {"enum": ["service.name", "service.version", super::telemetry::IDENTIFIER_ATTRIBUTE]}}, "additionalProperties": {"type": "string"}})
}

/// Reject whitespace and C0/C1 control characters, matching the runtime check.
/// The runtime bound is in bytes; `maxLength` here counts characters, so a
/// multi-byte identifier is checked exactly once, at startup.
pub(super) fn server_identifier(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"anyOf": [with_environment(json_schema!({
        "type": "string",
        "minLength": 1,
        "maxLength": super::MAX_SERVER_IDENTIFIER_BYTES,
        "pattern": "^[^\\s\\x00-\\x1f\\x7f-\\x9f]+$(?![\\s\\S])"
    })), {"type": "null"}]})
}

/// The published semantic version grammar. Clients compare major, minor, and
/// patch only, but the value must still parse in full.
pub(super) fn optional_semver(_: &mut SchemaGenerator) -> Schema {
    let core = "(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)";
    let prerelease = "(?:-(?:0|[1-9][0-9]*|[0-9]*[a-zA-Z-][0-9a-zA-Z-]*)(?:\\.(?:0|[1-9][0-9]*|[0-9]*[a-zA-Z-][0-9a-zA-Z-]*))*)?";
    let build = "(?:\\+[0-9a-zA-Z-]+(?:\\.[0-9a-zA-Z-]+)*)?";
    json_schema!({"anyOf": [with_environment(json_schema!({
        "type": "string",
        "pattern": format!("^{core}{prerelease}{build}$(?![\\s\\S])")
    })), {"type": "null"}]})
}
