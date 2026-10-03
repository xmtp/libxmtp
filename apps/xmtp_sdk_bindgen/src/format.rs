//! Format generated TypeScript without giving the formatter a path to read.

use std::{
    io::Write as _,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result, bail};
use camino::Utf8Path;

const FORMATTER: &str = "node_modules/.bin/oxfmt";
const CONFIG: &str = "apps/xmtp_sdk_bindgen/templates/bridge/oxfmt.json";

// Formatting only changes layout. A job without the JavaScript toolchain,
// such as the Windows load job, generates unformatted TypeScript instead.
static DISABLED: AtomicBool = AtomicBool::new(false);

/// Leave generated TypeScript unformatted for the rest of this process.
pub(crate) fn disable() {
    DISABLED.store(true, Ordering::Relaxed);
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
fn command(name: &str) -> Command {
    let mut command = Command::new(FORMATTER);
    command
        .arg("--config")
        .arg(CONFIG)
        .arg(format!("--stdin-filepath={name}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    command
}

/// Format `source` as the TypeScript file `name`, unless formatting is off.
pub(crate) fn typescript(name: &str, source: &str) -> Result<String> {
    typescript_with(!DISABLED.load(Ordering::Relaxed), name, source)
}

fn typescript_with(enabled: bool, name: &str, source: &str) -> Result<String> {
    if !enabled {
        return Ok(source.to_owned());
    }
    if !Utf8Path::new(FORMATTER).exists() {
        bail!("TypeScript formatter missing: run `just install` before SDK generation");
    }
    let mut child = command(name)
        .spawn()
        .with_context(|| format!("start the formatter for {name}"))?;
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

    // With formatting off, the source is returned unchanged and no formatter
    // is needed.
    #[xmtp_common::test(unwrap_try = true)]
    fn disabled_formatting_needs_no_formatter() {
        let source = "export const a=1\n";
        assert_eq!(typescript_with(false, "a.gen.ts", source)?, source);
    }

    // The formatter gets the source on standard input and no path to walk.
    #[xmtp_common::test(unwrap_try = true)]
    fn formatter_receives_no_path() {
        let command = command("client-forwarding.gen.ts");
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
