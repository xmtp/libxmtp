/// print bytes as a truncated hex string
pub fn debug_hex(bytes: impl AsRef<[u8]>) -> String {
    truncate_hex(hex::encode(bytes.as_ref()))
}

pub fn truncate_hex(hex_string: impl AsRef<str>) -> String {
    let hex_string = hex_string.as_ref();
    // If empty string, return it
    if hex_string.is_empty() {
        return String::new();
    }

    let hex_value = if let Some(hex_value) = hex_string.strip_prefix("0x") {
        hex_value
    } else {
        hex_string
    };

    // If the hex value is 8 or fewer chars, return original string
    if hex_value.len() <= 8 {
        return hex_string.to_string();
    }

    format!(
        "0x{}...{}",
        &hex_value[..4],
        &hex_value[hex_value.len() - 4..]
    )
}

pub trait TruncatedHex {
    fn short_hex(&self) -> String;
}
impl TruncatedHex for Vec<u8> {
    fn short_hex(&self) -> String {
        self.as_slice().short_hex()
    }
}
impl TruncatedHex for &[u8] {
    fn short_hex(&self) -> String {
        truncate_hex(hex::encode(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_long_hex() {
        assert_eq!(
            truncate_hex("0x5bf078bd83995fe83092d93c5655f059"),
            "0x5bf0...f059"
        );
    }
}
