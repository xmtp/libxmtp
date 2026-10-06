use anyhow::{Result, bail};

const NAME: &str = "sdkDiscardUnreturnedClient";

fn replace(source: &str, declaration: &str, internal: &str) -> Result<String> {
    if !source.contains(NAME) {
        return Ok(source.to_owned());
    }
    if source.matches(declaration).count() != 1 {
        bail!("private Client discard declaration changed");
    }
    Ok(source.replacen(declaration, internal, 1))
}

pub fn swift(source: &str) -> Result<String> {
    let source = stream_owner(source, true)?;
    replace(
        &source,
        "public func sdkDiscardUnreturnedClient(",
        "internal func sdkDiscardUnreturnedClient(",
    )
}

pub fn kotlin(source: &str) -> Result<String> {
    let source = stream_owner(source, false)?;
    replace(
        &source,
        "suspend fun `sdkDiscardUnreturnedClient`(",
        "internal suspend fun `sdkDiscardUnreturnedClient`(",
    )
}

// The owner key is a binding detail. Remove it from the public protocols and
// keep the concrete method internal for the host runtime.
fn stream_owner(source: &str, swift: bool) -> Result<String> {
    let (declaration, method, internal) = if swift {
        (
            "func sdkStreamOwnerKey(",
            "open func sdkStreamOwnerKey(",
            "internal func sdkStreamOwnerKey(",
        )
    } else {
        (
            "fun `sdkStreamOwnerKey`(",
            "override fun `sdkStreamOwnerKey`(",
            "internal open fun `sdkStreamOwnerKey`(",
        )
    };
    if !source.contains("sdkStreamOwnerKey") {
        return Ok(source.to_owned());
    }
    if source
        .lines()
        .filter(|line| line.trim().starts_with(declaration))
        .count()
        != 3
        || source.matches(method).count() != 3
    {
        bail!("private stream owner declarations changed");
    }
    Ok(source
        .lines()
        .filter(|line| !line.trim().starts_with(declaration))
        .map(|line| format!("{}\n", line.replace(method, internal)))
        .collect())
}
