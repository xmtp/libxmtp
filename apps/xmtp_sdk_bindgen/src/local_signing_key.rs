//! Native language conformance for the Rust-backed local signing key.
use anyhow::{Context, Result, ensure};

pub(crate) fn swift(source: &str) -> String {
    format!(
        "{source}\n// The Rust-backed key can be passed directly to client creation.\nextension LocalSigningKey: Signer {{}}\n"
    )
}

pub(crate) fn kotlin(source: &str) -> Result<String> {
    let header = "public interface LocalSigningKeyInterface {";
    ensure!(
        source.matches(header).count() == 1,
        "expected one local signing key interface"
    );
    let start = source
        .find(header)
        .context("local signing key interface missing")?;
    let end = start
        + source[start..]
            .find('}')
            .context("local signing key interface is not closed")?;
    let mut interface = source[start..end].replace(
        header,
        "public interface LocalSigningKeyInterface : Signer {",
    );
    for name in ["identity", "kind", "sign"] {
        let method = format!("suspend fun `{name}`(");
        ensure!(
            interface.matches(&method).count() == 1,
            "local signing key {name} method changed"
        );
        interface = interface.replacen(&method, &format!("override {method}"), 1);
    }
    Ok(format!("{}{interface}{}", &source[..start], &source[end..]))
}
