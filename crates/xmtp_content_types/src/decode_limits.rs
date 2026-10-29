//! Element counts checked before a codec builds decoded content.

use prost::encoding::{DecodeContext, decode_key, skip_field};

use crate::CodecError;

/// Top-level fields allowed in a nested `EncodedContent` or a `MultiRemoteAttachment`.
pub const MAX_PROTOBUF_FIELDS: usize = 4096;
/// `[`, `{`, `,`, and `:` characters allowed outside JSON strings.
pub const MAX_JSON_STRUCTURE: usize = 16383;

/// Fails when protobuf bytes hold more than [`MAX_PROTOBUF_FIELDS`] top-level fields.
// implements: CTYPE-033
pub fn check_protobuf_fields(mut bytes: &[u8]) -> Result<(), CodecError> {
    let mut fields = 0;
    while !bytes.is_empty() {
        fields += 1;
        if fields > MAX_PROTOBUF_FIELDS {
            return Err(CodecError::Decode(format!(
                "content has more than {MAX_PROTOBUF_FIELDS} protobuf fields"
            )));
        }
        // Malformed bytes are left for prost to report.
        let Ok((tag, wire_type)) = decode_key(&mut bytes) else {
            return Ok(());
        };
        if skip_field(wire_type, tag, &mut bytes, DecodeContext::default()).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

/// Fails when JSON bytes hold more than [`MAX_JSON_STRUCTURE`] structure
/// characters outside strings.
// implements: CTYPE-033
pub fn check_json_values(bytes: &[u8]) -> Result<(), CodecError> {
    let mut count = 0;
    let mut in_string = false;
    let mut escaped = false;
    for &byte in bytes {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' | b',' | b':' => {
                count += 1;
                if count > MAX_JSON_STRUCTURE {
                    return Err(CodecError::Decode(format!(
                        "content has more than {MAX_JSON_STRUCTURE} JSON structure characters"
                    )));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

    // verifies: CTYPE-033
    #[xmtp_common::test(unwrap_try = true)]
    fn protobuf_field_count() {
        let fields = |n: usize| [0x08, 0x00].repeat(n);
        let cases: [(&str, Vec<u8>, bool); 4] = [
            ("at the limit", fields(MAX_PROTOBUF_FIELDS), true),
            ("over the limit", fields(MAX_PROTOBUF_FIELDS + 1), false),
            ("truncated key", vec![0xff], true),
            ("truncated length", vec![0x0a, 0x05, 0x01], true),
        ];
        for (case, bytes, ok) in cases {
            assert_eq!(check_protobuf_fields(&bytes).is_ok(), ok, "{case}");
        }
        // Malformed bytes reach prost, which reports its own error.
        assert!(EncodedContent::decode([0xff].as_slice()).is_err());
        assert!(EncodedContent::decode([0x0a, 0x05, 0x01].as_slice()).is_err());
    }

    // verifies: CTYPE-033
    #[xmtp_common::test(unwrap_try = true)]
    fn json_structure_count() {
        // `[` plus one comma per element before the last.
        let array = |n: usize| format!("[{}0", "0,".repeat(n - 1));
        let cases = [
            (
                "at the limit",
                format!("{}]", array(MAX_JSON_STRUCTURE)),
                true,
            ),
            (
                "over the limit",
                format!("{},0]", array(MAX_JSON_STRUCTURE)),
                false,
            ),
            (
                "structure inside a string, past an escaped quote",
                format!(
                    r#"{},"{}"]"#,
                    array(MAX_JSON_STRUCTURE - 1),
                    r#",{[:\"\\"#.repeat(8)
                ),
                true,
            ),
            (
                "an escaped backslash ends before the quote",
                format!(r#"{},"\\",0]"#, array(MAX_JSON_STRUCTURE - 1)),
                false,
            ),
        ];
        for (case, json, ok) in cases {
            assert_eq!(check_json_values(json.as_bytes()).is_ok(), ok, "{case}");
        }
    }
}
