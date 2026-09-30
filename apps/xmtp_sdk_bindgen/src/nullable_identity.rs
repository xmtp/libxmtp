use anyhow::{Result, bail};

/// Methods whose absent result is `null`, never `undefined`, in both
/// TypeScript APIs. An absent DM peer and an unknown received creator or
/// adder are loaded values, not missing ones.
pub(crate) const NULLABLE_RESULTS: &[(&str, &str)] = &[
    ("Dm", "peerInboxId"),
    ("Dm", "creatorInboxId"),
    ("Dm", "addedByInboxId"),
    ("Group", "creatorInboxId"),
    ("Group", "addedByInboxId"),
];

pub(crate) fn is_nullable(owner: &str, method: &str) -> bool {
    NULLABLE_RESULTS.contains(&(owner, method))
}

/// Rewrite the generated binding so each nullable method returns `null`.
pub(crate) fn rewrite(source: &str) -> Result<String> {
    let source = rewrite_peer(source)?;
    ["creatorInboxId", "addedByInboxId"]
        .into_iter()
        .try_fold(source, |source, name| rewrite_getter(&source, name))
}

fn rewrite_peer(source: &str) -> Result<String> {
    let declaration = "peerInboxId(asyncOpts_?: { signal: AbortSignal }) /*throws*/: Promise<InboxId | undefined>;";
    let method = "async peerInboxId(asyncOpts_?: { signal: AbortSignal }): Promise<InboxId | undefined> /*throws*/ {";
    if source.matches(declaration).count() != 1 || source.matches(method).count() != 1 {
        bail!("generated TypeScript DM peer signature changed");
    }
    let start = source.find(method).expect("checked method");
    let end = source[start + method.len()..]
        .find("\n    async ")
        .map(|offset| start + method.len() + offset)
        .ok_or_else(|| anyhow::anyhow!("generated TypeScript DM peer method boundary changed"))?;
    let body = &source[start..end];
    let lift = "return FfiConverterOptionalTypeInboxId.lift(__rb);";
    if body.matches(lift).count() != 1 {
        bail!("generated TypeScript DM peer lift changed");
    }
    let body = body
        .replace("Promise<InboxId | undefined>", "Promise<InboxId | null>")
        .replace(
            lift,
            "return FfiConverterOptionalTypeInboxId.lift(__rb) ?? null;",
        );
    Ok(format!("{}{}{}", &source[..start], body, &source[end..])
        .replace(declaration, &declaration.replace("undefined", "null")))
}

/// Group and Dm each declare and implement the synchronous getter once.
fn rewrite_getter(source: &str, name: &str) -> Result<String> {
    let declaration = format!("    {name}(): InboxId | undefined;\n");
    let method = format!("    {name}(): InboxId | undefined {{\n");
    let lift = "return FfiConverterOptionalTypeInboxId.lift(__rb);";
    if source.matches(&declaration).count() != 2 || source.matches(&method).count() != 2 {
        bail!("generated TypeScript {name} signature changed");
    }
    let mut output = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find(&method) {
        let end = rest[start..]
            .find("\n    }\n")
            .map(|offset| start + offset)
            .ok_or_else(|| anyhow::anyhow!("generated TypeScript {name} boundary changed"))?;
        let body = &rest[start..end];
        if body.matches(lift).count() != 1 {
            bail!("generated TypeScript {name} lift changed");
        }
        output.push_str(&rest[..start]);
        output.push_str(
            &body
                .replace("InboxId | undefined", "InboxId | null")
                .replace(
                    lift,
                    "return FfiConverterOptionalTypeInboxId.lift(__rb) ?? null;",
                ),
        );
        rest = &rest[end..];
    }
    output.push_str(rest);
    Ok(output.replace(&declaration, &format!("    {name}(): InboxId | null;\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn getter(name: &str) -> String {
        format!(
            "    {name}(): InboxId | undefined {{\n    const __rb = call();\n    try {{\n        return FfiConverterOptionalTypeInboxId.lift(__rb);\n    }} finally {{\n        free(__rb);\n    }}\n    }}\n"
        )
    }

    // Both objects' getters return null for an unknown value; other methods keep undefined.
    #[xmtp_common::test(unwrap_try = true)]
    fn received_identity_getters_return_null() {
        let object = format!(
            "    creatorInboxId(): InboxId | undefined;\n    addedByInboxId(): InboxId | undefined;\n{}{}    other(): string | undefined {{\n        return FfiConverterOptionalTypeInboxId.lift(__rb);\n    }}\n",
            getter("creatorInboxId"),
            getter("addedByInboxId")
        );
        let source = format!("{object}{object}");
        let mut output = source.clone();
        for name in ["creatorInboxId", "addedByInboxId"] {
            output = rewrite_getter(&output, name)?;
        }
        assert_eq!(output.matches("(): InboxId | null;").count(), 4);
        assert_eq!(output.matches("(): InboxId | null {").count(), 4);
        assert_eq!(output.matches("lift(__rb) ?? null;").count(), 4);
        assert_eq!(output.matches("other(): string | undefined {\n        return FfiConverterOptionalTypeInboxId.lift(__rb);\n").count(), 2);
        assert!(rewrite_getter(&object, "creatorInboxId").is_err());
    }
}
