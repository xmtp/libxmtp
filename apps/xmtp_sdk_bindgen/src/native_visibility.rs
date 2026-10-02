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
    replace(
        source,
        "public func sdkDiscardUnreturnedClient(",
        "internal func sdkDiscardUnreturnedClient(",
    )
}

pub fn kotlin(source: &str) -> Result<String> {
    replace(
        source,
        "suspend fun `sdkDiscardUnreturnedClient`(",
        "internal suspend fun `sdkDiscardUnreturnedClient`(",
    )
}
