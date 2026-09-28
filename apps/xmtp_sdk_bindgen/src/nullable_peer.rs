use anyhow::{Result, bail};

/// Keep the public DM peer absence equal to null in the Node API.
pub(crate) fn rewrite(source: &str) -> Result<String> {
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
