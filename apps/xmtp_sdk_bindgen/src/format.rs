//! Format generated TypeScript without giving the formatter a path to read.

use std::{
    io::Write as _,
    process::{Command, Stdio},
    sync::OnceLock,
};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};

const CONFIG: &str = "apps/xmtp_sdk_bindgen/templates/bridge/oxfmt.json";

// One `oxfmt` binary, which `render --oxfmt` or `XMTP_SDK_OXFMT` names, formats
// TypeScript in every route, so every route writes the same bytes.
static FORMATTER: OnceLock<Utf8PathBuf> = OnceLock::new();

/// Format generated TypeScript with this binary for the rest of the process.
pub(crate) fn use_formatter(path: Utf8PathBuf) {
    // `render` runs once per process, so the formatter is set at most once.
    let _ = FORMATTER.set(path);
}

/// Remove generated Swift line-end spaces before recording output hashes.
pub(crate) fn swift_trailing_whitespace(source: &str) -> String {
    source
        .split_inclusive('\n')
        .map(|line| {
            let (body, ending) = line
                .strip_suffix('\n')
                .map_or((line, ""), |body| (body, "\n"));
            format!("{}{ending}", body.trim_end_matches([' ', '\t']))
        })
        .collect()
}

/// The formatter command for one source. The source goes through standard
/// input, and `name` only selects the parser. The formatter reads no
/// directory, so it never walks a generated package's node_modules.
fn command(formatter: &Utf8Path, name: &str) -> Command {
    let mut command = Command::new(formatter);
    command
        .arg("--config")
        .arg(CONFIG)
        .arg(format!("--stdin-filepath={name}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    command
}

/// Format `source` as the TypeScript file `name`.
pub(crate) fn typescript(name: &str, source: &str) -> Result<String> {
    let formatter = FORMATTER
        .get()
        .context("TypeScript output needs oxfmt: pass --oxfmt or set XMTP_SDK_OXFMT")?;
    let mut child = command(formatter, name)
        .spawn()
        .with_context(|| format!("start the formatter {formatter} for {name}"))?;
    let mut input = child.stdin.take().context("formatter input")?;
    let source = source.to_owned();
    // Write from another thread, so a large source cannot block on a full
    // output pipe.
    let writer = std::thread::spawn(move || input.write_all(source.as_bytes()));
    let output = child
        .wait_with_output()
        .with_context(|| format!("run the formatter for {name}"))?;
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("formatter input thread panicked"))?
        .with_context(|| format!("write {name} to the formatter"))?;
    if !output.status.success() {
        bail!("formatter failed for {name}: {}", output.status);
    }
    String::from_utf8(output.stdout).with_context(|| format!("formatter output for {name}"))
}

/// Format each generated file in place, through standard input.
pub(crate) fn typescript_files<'a>(paths: impl IntoIterator<Item = &'a Utf8Path>) -> Result<()> {
    for path in paths {
        let name = path.file_name().context("generated file has no name")?;
        let source = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        std::fs::write(path, typescript(name, &source)?)
            .with_context(|| format!("write {path}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn swift_line_end_cleanup_preserves_code_and_is_idempotent() {
        let source = "  let value = \"inside  \"  \n \t\n\t// comment\t\nlast  ";
        let expected = "  let value = \"inside  \"\n\n\t// comment\nlast";
        let output = swift_trailing_whitespace(source);
        assert_eq!(output, expected);
        assert_eq!(swift_trailing_whitespace(&output), output);
        assert_eq!(swift_trailing_whitespace("let x = 1\n"), "let x = 1\n");
        assert_eq!(swift_trailing_whitespace(""), "");
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn typescript_output_needs_a_formatter() {
        let error = typescript("index.ts", "export const a=1\n").unwrap_err();
        assert!(error.to_string().contains("needs oxfmt"), "{error}");
    }

    // The formatter gets the source on standard input and no path to walk.
    #[xmtp_common::test(unwrap_try = true)]
    fn formatter_receives_no_path() {
        let command = command(Utf8Path::new("oxfmt"), "client-forwarding.gen.ts");
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            vec![
                "--config",
                CONFIG,
                "--stdin-filepath=client-forwarding.gen.ts"
            ]
        );
    }
}
