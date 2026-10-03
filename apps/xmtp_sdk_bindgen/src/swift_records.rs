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
