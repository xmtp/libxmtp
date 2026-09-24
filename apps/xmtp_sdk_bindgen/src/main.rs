mod validate;

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand, ValueEnum};
use regex::Regex;
use ubrn_bindgen::{
    AbiFlavor, BindingsArgs, OutputArgs, SourceArgs, SwitchArgs,
    ffi_module_player_lib_resolution::LibResolution, wasm_metadata,
};
use uniffi_bindgen::{BindgenLoader, BindgenPaths, GlobalConfig, bindings};

#[derive(Parser)]
#[command(about = "Generate XMTP SDK bindings from a UniFFI library")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Generate {
        #[arg(long)]
        lib: Utf8PathBuf,
        #[arg(long, value_enum)]
        language: Language,
        #[arg(long)]
        out: Utf8PathBuf,
        #[arg(long)]
        config: Option<Utf8PathBuf>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Language {
    Swift,
    Kotlin,
    TypescriptNapi,
    TypescriptWasm,
}

fn main() -> Result<()> {
    let Cli { command } = Cli::parse();
    match command {
        Command::Generate {
            lib,
            language,
            out,
            config,
        } => generate(&lib, language, &out, config.as_deref()),
    }
}

fn generate(
    lib: &Utf8Path,
    language: Language,
    out: &Utf8Path,
    config: Option<&Utf8Path>,
) -> Result<()> {
    let default_config = Utf8Path::new("apps/xmtp_sdk_bindgen/uniffi-global.toml");
    let config = config.unwrap_or(default_config);
    let (global_config, roots_layer) = GlobalConfig::from_file(config)?;
    let mut paths = BindgenPaths::default();
    if let Some(layer) = roots_layer {
        paths.add_layer(layer);
    }
    let _crate_root = paths
        .get_crate_root("xmtp_sdk")
        .context("global config needs [crate-roots] xmtp_sdk")?;
    let loader = BindgenLoader::new(paths, global_config);
    let metadata = loader.load_metadata_specialized(lib, |_path, bytes| {
        if wasm_metadata::looks_like_wasm(bytes) {
            Ok(Some(wasm_metadata::extract_from_wasm_bytes(bytes)?))
        } else {
            Ok(None)
        }
    })?;
    validate::validate_metadata(&metadata)?;
    fs::create_dir_all(out)?;

    match language {
        Language::Swift | Language::Kotlin => {
            if wasm_metadata::looks_like_wasm(&fs::read(lib)?) {
                bail!("{}: Swift and Kotlin need a native library", lib);
            }
            bindings::generate(bindings::GenerateOptions {
                languages: vec![match language {
                    Language::Swift => bindings::TargetLanguage::Swift,
                    _ => bindings::TargetLanguage::Kotlin,
                }],
                source: lib.to_owned(),
                out_dir: out.to_owned(),
                config_override: Some(config.to_owned()),
                format: false,
                crate_filter: Some("xmtp_sdk".into()),
                metadata_no_deps: true,
            })?;
        }
        Language::TypescriptNapi | Language::TypescriptWasm => {
            let is_wasm = matches!(language, Language::TypescriptWasm);
            if is_wasm != wasm_metadata::looks_like_wasm(&fs::read(lib)?) {
                bail!(
                    "{}: library format does not match the TypeScript target",
                    lib
                );
            }
            let flavor = if is_wasm {
                AbiFlavor::Wasm2
            } else {
                AbiFlavor::Napi
            };
            // The fork's TypeScript config rejects UniFFI's rename table.
            // Select a component-scoped config for this backend.
            let ts_config = config
                .parent()
                .context("global config has no parent directory")?
                .join("typescript.toml");
            let source = SourceArgs::library(&lib.to_owned()).with_config(Some(ts_config));
            let mut args = BindingsArgs::new(
                SwitchArgs { flavor },
                source,
                OutputArgs::new(out, &out.join("abi"), true),
            );
            if !is_wasm {
                args = args.with_lib_resolution(LibResolution::Colocated);
            }
            // The fork asks cargo for a manifest even when the source is a library.
            // This small workspace keeps generation independent of Cargo resolution.
            let manifest_dir = out.join(".bindgen-manifest");
            fs::create_dir_all(manifest_dir.join("src"))?;
            fs::write(
                manifest_dir.join("Cargo.toml"),
                "[package]\nname = \"xmtp-sdk-bindgen-input\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[workspace]\n",
            )?;
            fs::write(manifest_dir.join("src/lib.rs"), "")?;
            let manifest = manifest_dir.join("Cargo.toml");
            args.run(Some(&manifest))?;
            normalize_typescript_ids(out)?;
        }
    }

    let runtime_name = match language {
        Language::Swift => "swift",
        Language::Kotlin => "kotlin",
        Language::TypescriptNapi | Language::TypescriptWasm => "ts",
    };
    let runtime = config
        .parent()
        .context("global config has no parent directory")?
        .join("runtime")
        .join(runtime_name);
    copy_tree(runtime.as_std_path(), out.join("runtime").as_std_path())?;
    Ok(())
}

fn normalize_typescript_ids(out: &Utf8Path) -> Result<()> {
    let id_suffix = Regex::new(r"\b[A-Za-z_][A-Za-z0-9_]*Ids?\b")?;
    for entry in fs::read_dir(out)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "ts") {
            let source = fs::read_to_string(&path)?;
            let normalized = id_suffix.replace_all(&source, |captures: &regex::Captures<'_>| {
                let name = &captures[0];
                if let Some(prefix) = name.strip_suffix("Ids") {
                    format!("{prefix}IDs")
                } else {
                    format!("{}ID", name.strip_suffix("Id").expect("ID suffix"))
                }
            });
            fs::write(path, normalized.as_bytes())?;
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source).with_context(|| format!("read {}", source.display()))? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
