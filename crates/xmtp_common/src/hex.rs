pub trait NormalizeHex {
    fn normalize_hex(&self) -> String;
}

impl NormalizeHex for str {
    fn normalize_hex(&self) -> String {
        let lower = self.to_lowercase();
        lower.strip_prefix("0x").unwrap_or(&lower).to_string()
    }
}

impl NormalizeHex for String {
    fn normalize_hex(&self) -> String {
        self.as_str().normalize_hex()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_hex_str_with_mixed_case_prefix() {
        assert_eq!("0xABCDEF".normalize_hex(), "abcdef");
        assert_eq!("0XAbCdEf".normalize_hex(), "abcdef");
        assert_eq!("0xAbC123".normalize_hex(), "abc123");
    }

    #[test]
    fn test_normalize_hex_str_without_prefix() {
        assert_eq!("abcdef".normalize_hex(), "abcdef");
        assert_eq!("123456".normalize_hex(), "123456");
        assert_eq!("ABCDEF".normalize_hex(), "abcdef");
        assert_eq!("AbCdEf".normalize_hex(), "abcdef");
    }

    #[test]
    fn test_normalize_hex_str_already_normalized() {
        assert_eq!("abcdef123456".normalize_hex(), "abcdef123456");
        assert_eq!("0".normalize_hex(), "0");
        assert_eq!("ff".normalize_hex(), "ff");
    }

    #[test]
    fn test_normalize_hex_str_edge_cases() {
        assert_eq!("".normalize_hex(), "");
        assert_eq!("0x".normalize_hex(), "");
        assert_eq!("0X".normalize_hex(), "");
        assert_eq!("x".normalize_hex(), "x");
        assert_eq!("0".normalize_hex(), "0");
    }
}
