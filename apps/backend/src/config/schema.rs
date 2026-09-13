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

/// Match the chain parser's unsigned integer range, including leading zeros and a plus sign.
pub(super) fn chains(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "object",
        "propertyNames": {"pattern": format!("^eip155:\\+?0*{}$(?![\\s\\S])", decimal_range(u64::MAX))},
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

/// Encode the standard library socket grammar with portable JSON Schema patterns.
/// IPv6 compression omits at least one group. An embedded IPv4 address uses two groups.
pub(super) fn socket_address(_: &mut SchemaGenerator) -> Schema {
    let octet = "(0|[1-9][0-9]?|1[0-9]{2}|2[0-4][0-9]|25[0-5])";
    let ipv4 = [octet; 4].join("\\.");
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
    let scope = decimal_range(u32::MAX as u64);
    let port = decimal_range(u16::MAX as u64);
    with_environment(json_schema!({
        "type": "string",
        "pattern": format!("^({ipv4}|\\[({})(%0*{scope})?\\]):0*{port}$(?![\\s\\S])", ipv6.join("|"))
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
    json_schema!({"type": "object", "propertyNames": {"not": {"enum": ["service.name", "service.version"]}}, "additionalProperties": {"type": "string"}})
}
