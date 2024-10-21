use crate::flags;
use color_eyre::eyre::Result;
use spinach::Spinner;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use xshell::{cmd, Shell};

pub const BINDINGS_WASM: &str = "bindings_wasm";

pub fn build(extra_args: &[String], flags: flags::Build) -> Result<()> {
    match flags.subcommand {
        flags::BuildCmd::BindingsWasm(f) => build_bindings_wasm(extra_args, f)?,
    }
    Ok(())
}

pub fn build_bindings_wasm(extra_args: &[String], flags: flags::BindingsWasm) -> Result<()> {
    let sp = Spinner::new("Building bindings wasm").start();

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

    let package_json = r#"
    {
        "dependencies": {
            "@sqlite.org/sqlite-wasm": "latest"
        }
    }
    "#;
    let spinner_update = |s: &str| sp.text(s).update();

    cargo_build(extra_args, spinner_update)?;
    create_pkg_dir(&pkg_directory, spinner_update)?;
    sp.text("writing package.json").update();
    fs::write(pkg_directory.join("package.json"), package_json)?;

    sp.text("copying readme").update();
    fs::copy(
        manifest_dir.join("README.md"),
        pkg_directory.join("README.md"),
    )?;
    sp.text("copying license").update();
    fs::copy(manifest_dir.join("LICENSE"), pkg_directory.join("LICENSE"))?;

    sp.text("running wasm-bindgen").update();
    step_wasm_bindgen_build(&wasm_path, &pkg_directory, spinner_update)?;

    sp.text("running wasm-opt").update();
    step_run_wasm_opt(&pkg_directory, spinner_update)?;
    sp.success();
    Ok(())
}

pub fn cargo_build<T>(extra_args: &[String], f: impl Fn(&str) -> T) -> Result<()> {
    let sh = xshell::Shell::new()?;
    sh.change_dir(std::env!("CARGO_MANIFEST_DIR"));
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
    // let _env = sh.push_env("RUSTFLAGS", crate::RUSTFLAGS);
    let cmd = cmd!(sh, "wasm-bindgen {wasm_path} --out-dir {pkg_directory} --typescript --target web --split-linked-modules");
    pretty_print(cmd, f)?;
    Ok(())
}

/// Construct our `pkg` directory in the crate.
pub fn create_pkg_dir<T>(out_dir: &Path, f: impl Fn(&str) -> T) -> Result<()> {
    f(&format!("creating package directory {}", out_dir.display()));
    let _ = fs::remove_file(out_dir.join("package.json")); // Clean up package.json from previous runs
    fs::create_dir_all(out_dir)?;
    fs::write(out_dir.join(".gitignore"), "*")?;
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
    // TODO: Check for ``wasm-opt` on `PATH`
    for file in out_dir.read_dir()? {
        let file = file?;
        let path = file.path();
        if path.extension().and_then(|s| s.to_str()) != Some("wasm") {
            continue;
        }

        let sh = Shell::new()?;
        let tmp = path.with_extension("wasm-opt.wasm");
        let mut cmd = cmd!(sh, "wasm-opt {path} -o {tmp} -Oz");
        println!("\n{cmd}");
        cmd.set_quiet(true);
        cmd.run()?;
        std::fs::rename(&tmp, &path)?;
    }

    Ok(())
}

/// Pretty print a cargo command with Spinach
fn pretty_print<T>(cmd: xshell::Cmd, f: impl Fn(&str) -> T) -> Result<()> {
    let mut child = Command::from(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(s) = child.stderr.take() {
        let mut reader = std::io::BufReader::new(s);
        while let Ok(None) = child.try_wait() {
            let mut buf = String::new();
            reader.read_line(&mut buf)?;
            if !buf.is_empty() {
                f(buf.trim());
            }
        }
    }
    Ok(())
}
