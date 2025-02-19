pub fn truncate_hex(hex_string: impl AsRef<str>) -> String {
    let hex_string = hex_string.as_ref();
    // If empty string, return it
    if hex_string.is_empty() {
        return String::new();
    }

    // Determine if string has 0x prefix
    let hex_value = if hex_string.starts_with("0x") {
        &hex_string[2..]
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
