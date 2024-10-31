use crate::flags;
use color_eyre::{
    eyre::Result,
    owo_colors::{colors::*, OwoColorize},
};
use spinach::Spinner;
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use xshell::{cmd, Shell};

pub const BINDINGS_WASM: &str = "bindings_wasm";

pub fn build(extra_args: &[String], flags: flags::Build) -> Result<()> {
    let sp = Spinner::new("building");
    let mut sp_running = None;
    let text_update = if flags.plain {
        Box::new(|s: &str| {
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(s.as_bytes());
        }) as Box<dyn Fn(&str)>
    } else {
        sp_running = Some(sp.start());
        Box::new(|s: &str| {
            sp_running.as_ref().unwrap().text(s.trim()).update();
        }) as Box<dyn Fn(&str)>
    };

    let res = match flags.subcommand {
        flags::BuildCmd::BindingsWasm(f) => build_bindings_wasm(extra_args, f, &text_update),
    };

    drop(text_update);
    if let Some(sp) = sp_running {
        match res {
            Ok(_) => sp.text("Wasm build success").success(),
            Err(e) => sp.text(&format!("{}", e)).failure(),
        }
    } else {
        res?
    }

    Ok(())
}

pub fn build_bindings_wasm(
    extra_args: &[String],
    flags: flags::BindingsWasm,
    f: &impl Fn(&str),
) -> Result<()> {
    f("building bindings wasm\n");

    let workspace_dir = workspace_dir()?;
    let manifest_dir = workspace_dir.join(BINDINGS_WASM);

    let pkg_directory = manifest_dir
        .clone()
        .join(flags.out_dir.unwrap_or("dist".into()));

    let target_directory = {
        let mut has_target_dir_iter = extra_args.iter();
        has_target_dir_iter
            .find(|&it| it == "--target-dir")
            .and_then(|_| has_target_dir_iter.next())
            .map(PathBuf::from)
            .unwrap_or(workspace_dir.join("target"))
    };

    let release_or_debug = {
        let mut has_release = extra_args.iter();
        if has_release.any(|it| it == "--release") {
            "release"
        } else {
            "debug"
        }
    };

    let wasm_path = target_directory
        .join("wasm32-unknown-unknown")
        .join(release_or_debug)
        .join(BINDINGS_WASM.replace("-", "_"))
        .with_extension("wasm");

    cargo_build(extra_args, f)?;
    create_pkg_dir(&pkg_directory, f)?;

    f("running wasm-bindgen");
    step_wasm_bindgen_build(&wasm_path, &pkg_directory, f)?;

    f("running wasm-opt");
    step_run_wasm_opt(&pkg_directory, f)?;
    Ok(())
}

pub fn cargo_build<T>(extra_args: &[String], f: impl Fn(&str) -> T) -> Result<()> {
    let sh = xshell::Shell::new()?;
    sh.change_dir(std::env!("CARGO_MANIFEST_DIR"));
    let _env = sh.push_env("RUSTFLAGS", crate::WASM_RUSTFLAGS);
    let cmd = cmd!(
        sh,
        "cargo build -p {BINDINGS_WASM} --target wasm32-unknown-unknown {extra_args...}"
    );
    pretty_print(cmd, f)?;

    Ok(())
}

pub fn step_wasm_bindgen_build<T>(
    wasm_path: &Path,
    pkg_directory: &Path,
    f: impl Fn(&str) -> T,
) -> Result<()> {
    // TODO: Check for wasm-bindgen on `PATH`
    let sh = Shell::new()?;
    let _env = sh.push_env("RUSTFLAGS", crate::WASM_RUSTFLAGS);
    let cmd = cmd!(sh, "wasm-bindgen {wasm_path} --out-dir {pkg_directory} --typescript --target web --split-linked-modules");
    pretty_print(cmd, f)?;
    Ok(())
}

/// Construct our `pkg` directory in the crate.
pub fn create_pkg_dir<T>(out_dir: &Path, f: impl Fn(&str) -> T) -> Result<()> {
    f(&format!("creating package directory {}", out_dir.display()));
    let _ = fs::remove_file(out_dir.join("package.json")); // Clean up package.json from previous runs
    fs::create_dir_all(out_dir)?;
    Ok(())
}

fn workspace_dir() -> Result<PathBuf> {
    let sh = Shell::new()?;
    let output = cmd!(
        sh,
        "cargo locate-project --workspace --message-format=plain"
    )
    .output()?
    .stdout;
    let cargo_path = Path::new(std::str::from_utf8(&output).unwrap().trim());
    Ok(cargo_path.parent().unwrap().to_path_buf())
}

pub fn step_run_wasm_opt<T>(out_dir: &Path, _f: impl Fn(&str) -> T) -> Result<()> {
    // TODO: Check for `wasm-opt` on `PATH`
    for file in out_dir.read_dir()? {
        let file = file?;
        let path = file.path();
        if path.extension().and_then(|s| s.to_str()) != Some("wasm") {
            continue;
        }

        let sh = Shell::new()?;
        let tmp = path.with_extension("wasm-opt.wasm");
        // use `wasm-opt` installed via `yarn`
        let mut cmd = cmd!(sh, "yarn wasm-opt {path} -o {tmp} -Oz");
        println!("\n{cmd}");
        cmd.set_quiet(true);
        if let Err(e) = cmd.run() {
            println!("{} {}", "Error".fg::<Yellow>(), e.fg::<Yellow>());
            println!(
                "{}",
                "Error optimizing with `wasm_opt`, leaving binary alone".fg::<Yellow>()
            );
        } else {
            std::fs::rename(&tmp, &path)?;
        }
    }

    Ok(())
}

/// Pretty print a cargo command with Spinach
fn pretty_print<T>(cmd: xshell::Cmd, f: impl Fn(&str) -> T) -> Result<()> {
    let mut child = Command::from(cmd)
        .env("CARGO_TERM_COLOR", "always")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(s) = child.stderr.take() {
        let mut reader = std::io::BufReader::new(s);
        while let Ok(None) = child.try_wait() {
            let mut buf = String::new();
            reader.read_line(&mut buf)?;
            if !buf.is_empty() {
                f(&buf);
            }
        }
    }
    Ok(())
}
