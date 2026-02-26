/// Information about a crate containing ErrorCode types.
pub struct CrateInfo {
    pub name: String,
    pub error_types: Vec<ErrorType>,
}

/// An enum or struct that derives ErrorCode.
pub struct ErrorType {
    pub name: String,
    pub kind: ErrorTypeKind,
    pub source_file: String,
    pub doc_comment: Option<String>,
    pub internal: bool,
    pub variants: Vec<ErrorVariant>,
}

pub enum ErrorTypeKind {
    Enum,
    Struct,
}

/// A single variant (for enums) or the struct itself.
pub struct ErrorVariant {
    pub error_code: String,
    pub doc_comment: Option<String>,
    pub inherit: bool,
}
