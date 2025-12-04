/// A trait for errors that expose a stable code string (e.g., `Enum::Variant`).
///
/// Use `#[derive(xmtp_common::ErrorCode)]` to auto-generate implementations.
pub trait ErrorCode: std::error::Error {
    fn error_code(&self) -> &'static str;
}

impl<T: ErrorCode> ErrorCode for Box<T> {
    fn error_code(&self) -> &'static str {
        (**self).error_code()
    }
}

impl ErrorCode for hex::FromHexError {
    fn error_code(&self) -> &'static str {
        "Hex::FromHexError"
    }
}

impl ErrorCode for prost::EncodeError {
    fn error_code(&self) -> &'static str {
        "Prost::EncodeError"
    }
}

impl ErrorCode for prost::DecodeError {
    fn error_code(&self) -> &'static str {
        "Prost::DecodeError"
    }
}

impl ErrorCode for xmtp_cryptography::signature::IdentifierValidationError {
    fn error_code(&self) -> &'static str {
        "IdentifierValidationError"
    }
}
