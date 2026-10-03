use anyhow::{Context, Result, ensure};

/// Keep the retained LeaveRequest constructor's empty-note value semantics.
pub(crate) fn rewrite(source: &str) -> Result<String> {
    let start = source
        .find("public struct LeaveRequest:")
        .context("generated Swift LeaveRequest record was not found")?;
    let end = source[start..]
        .find("\n}")
        .map(|at| start + at)
        .context("generated Swift LeaveRequest record has no end")?;
    let record = &source[start..end];
    let old = "public init(authenticatedNote: Data?) {\n        self.authenticatedNote = authenticatedNote";
    ensure!(
        record.matches(old).count() == 1,
        "generated Swift LeaveRequest constructor changed"
    );
    let updated = record.replace(old,
        "public init(authenticatedNote: Data? = nil) {\n        self.authenticatedNote = authenticatedNote?.isEmpty == true ? nil : authenticatedNote");
    let mut output = source.to_owned();
    output.replace_range(start..end, &updated);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD: &str = "public struct LeaveRequest: Equatable {\n    public var authenticatedNote: Data?\n    public init(authenticatedNote: Data?) {\n        self.authenticatedNote = authenticatedNote\n    }\n}";

    #[xmtp_common::test(unwrap_try = true)]
    fn leave_request_constructor_rewrite_is_scoped_and_normalizes_empty_data() {
        let other = "public struct Other {\n    public init(authenticatedNote: Data?) {\n        self.authenticatedNote = authenticatedNote\n    }\n}";
        let source = format!("{other}\n{RECORD}\n{other}");
        let output = rewrite(&source)?;
        assert_eq!(output.matches(other).count(), 2);
        assert_eq!(output.matches("authenticatedNote: Data? = nil").count(), 1);
        assert!(output.contains("authenticatedNote?.isEmpty == true ? nil : authenticatedNote"));
        assert!(!output.contains(RECORD));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn leave_request_constructor_rewrite_rejects_missing_changed_or_duplicate_constructor() {
        assert!(rewrite("public struct Other {}\n").is_err());
        assert!(rewrite(&RECORD.replace("Data?)", "Data)")).is_err());
        let repeated = RECORD.replace("\n}", "\n    public init(authenticatedNote: Data?) {\n        self.authenticatedNote = authenticatedNote\n    }\n}");
        assert!(rewrite(&repeated).is_err());
        assert!(rewrite("public struct LeaveRequest: Equatable {").is_err());
    }
}
