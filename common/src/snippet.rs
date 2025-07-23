/// Length for snippet truncation
pub const SNIPPET_LENGTH: usize = 6;

/// Trait for creating short, truncated representations of values for display/debugging
pub trait Snippet {
    /// Returns a short, truncated representation of the value
    fn snippet(&self) -> String;
}

impl Snippet for str {
    fn snippet(&self) -> String {
        if self.len() <= SNIPPET_LENGTH {
            self.to_string()
        } else {
            format!("{}..", &self[..SNIPPET_LENGTH])
        }
    }
}

impl Snippet for [u8] {
    fn snippet(&self) -> String {
        let encoded = hex::encode(self);
        if encoded.len() <= SNIPPET_LENGTH {
            encoded
        } else {
            format!("{}..", &encoded[..SNIPPET_LENGTH])
        }
    }
}

impl Snippet for Vec<u8> {
    fn snippet(&self) -> String {
        self.as_slice().snippet()
    }
}

impl Snippet for String {
    fn snippet(&self) -> String {
        self.as_str().snippet()
    }
}

impl<T: Snippet> Snippet for Option<T> {
    fn snippet(&self) -> String {
        match self {
            Some(value) => value.snippet(),
            None => "".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_str_snippet() {
        assert_eq!("hello".snippet(), "hello");
        assert_eq!("hello world".snippet(), "hello ..");
        assert_eq!("".snippet(), "");
        assert_eq!("a".snippet(), "a");
    }

    #[test]
    fn test_bytes_snippet() {
        let short_bytes = vec![1, 2, 3];
        let long_bytes = vec![1, 2, 3, 4, 5, 6, 7, 8];

        assert_eq!(short_bytes.snippet(), "010203");
        assert_eq!(long_bytes.snippet(), "010203..");
        assert_eq!([].snippet(), "");
    }

    #[test]
    fn test_string_snippet() {
        let short_string = String::from("hello");
        let long_string = String::from("hello world");

        assert_eq!(short_string.snippet(), "hello");
        assert_eq!(long_string.snippet(), "hello ..");
    }

    #[test]
    fn test_option_snippet() {
        let some_string: Option<String> = Some("hello world".to_string());
        let none_string: Option<String> = None;
        let some_bytes: Option<Vec<u8>> = Some(vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let none_bytes: Option<Vec<u8>> = None;

        assert_eq!(some_string.snippet(), "hello ..");
        assert_eq!(none_string.snippet(), "");
        assert_eq!(some_bytes.snippet(), "010203..");
        assert_eq!(none_bytes.snippet(), "");
    }
}
