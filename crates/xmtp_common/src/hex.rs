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
    fn test_normalize_hex_str() {
        assert_eq!("0xABCDEF".normalize_hex(), "abcdef");
        assert_eq!("0XAbCdEf".normalize_hex(), "abcdef");
        assert_eq!("0xAbC123".normalize_hex(), "abc123");
        assert_eq!("123456".normalize_hex(), "123456");
        assert_eq!("ABCDEF".normalize_hex(), "abcdef");
        assert_eq!("AbCdEf".normalize_hex(), "abcdef");
        assert_eq!("".normalize_hex(), "");
        assert_eq!("0x".normalize_hex(), "");
        assert_eq!("0X".normalize_hex(), "");
        assert_eq!("x".normalize_hex(), "x");
        assert_eq!("0".normalize_hex(), "0");
        assert_eq!(String::from("0XAbCdEf").normalize_hex(), "abcdef");
    }
}
