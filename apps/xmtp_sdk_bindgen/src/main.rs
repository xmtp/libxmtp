mod bridge;
mod callback_cursor;
mod forwarding;
mod id_names;
mod kotlin_callbacks;
mod kotlin_records;
mod validate;

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand, ValueEnum};
use ubrn_bindgen::{
    AbiFlavor, BindingsArgs, OutputArgs, SourceArgs, SwitchArgs,
    ffi_module_player_lib_resolution::LibResolution, wasm_metadata,
};
use uniffi_bindgen::{BindgenLoader, BindgenPaths, GlobalConfig, bindings};
use uniffi_meta::{Metadata, MetadataGroupMap};

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
        #[arg(long)]
        pure_only: bool,
    },
    StageWasm {
        #[arg(long)]
        lib: Utf8PathBuf,
        #[arg(long)]
        out: Utf8PathBuf,
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
            pure_only,
        } => generate(&lib, language, &out, config.as_deref(), pure_only),
        Command::StageWasm { lib, out } => {
            fs::create_dir_all(&out)?;
            ubrn_common::stage_wasm(&lib, &out, "xmtp_sdk", false)?;
            Ok(())
        }
    }
}

fn generate(
    lib: &Utf8Path,
    language: Language,
    out: &Utf8Path,
    config: Option<&Utf8Path>,
    pure_only: bool,
) -> Result<()> {
    let default_config = Utf8Path::new("apps/xmtp_sdk_bindgen/uniffi-global.toml");
    let config = config.unwrap_or(default_config);
    let (global_config, roots_layer) = GlobalConfig::from_file(config)?;
    let mut paths = BindgenPaths::default();
    if let Some(layer) = roots_layer {
        paths.add_layer(layer);
    }
    let crate_root = paths
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
    if pure_only {
        if !matches!(language, Language::TypescriptWasm) {
            bail!("pure-only generation requires typescript-wasm");
        }
        validate_pure_module(&metadata)?;
    }
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
            if matches!(language, Language::Kotlin) {
                let binding = out.join("uniffi/xmtp_sdk/xmtp_sdk.kt");
                let callbacks = kotlin_callbacks::rewrite(&fs::read_to_string(&binding)?)?;
                fs::write(&binding, kotlin_records::rewrite(&callbacks, &metadata)?)?;
            }
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
            let scratch = tempfile::tempdir()?;
            let scratch_path = Utf8Path::from_path(scratch.path())
                .context("bindgen scratch directory is not UTF-8")?;
            let mut ts_value: toml::Value = toml::from_str(&fs::read_to_string(&ts_config)?)?;
            let crate_value: toml::Value =
                toml::from_str(&fs::read_to_string(crate_root.join("uniffi.toml"))?)?;
            let custom_types = crate_value
                .get("bindings")
                .and_then(|value| value.get("typescript"))
                .and_then(|value| value.get("customTypes"))
                .context("crate config needs TypeScript custom types")?
                .clone();
            ts_value
                .get_mut("bindings")
                .and_then(|value| value.get_mut("typescript"))
                .and_then(toml::Value::as_table_mut)
                .context("TypeScript config needs [bindings.typescript]")?
                .insert("customTypes".into(), custom_types);
            let scoped_config = scratch_path.join("typescript.toml");
            fs::write(&scoped_config, toml::to_string(&ts_value)?)?;
            let source = SourceArgs::library(&lib.to_owned()).with_config(Some(scoped_config));
            let mut args = BindingsArgs::new(
                SwitchArgs { flavor },
                source,
                OutputArgs::new(out, &scratch_path.join("abi"), true),
            );
            if !is_wasm {
                args = args.with_lib_resolution(LibResolution::Colocated);
            }
            // The fork asks cargo for a manifest even when the source is a library.
            // This small workspace keeps generation independent of Cargo resolution.
            let manifest_dir = scratch_path.join("manifest");
            fs::create_dir_all(manifest_dir.join("src"))?;
            fs::write(
                manifest_dir.join("Cargo.toml"),
                "[package]\nname = \"xmtp-sdk-bindgen-input\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[workspace]\n",
            )?;
            fs::write(manifest_dir.join("src/lib.rs"), "")?;
            let manifest = manifest_dir.join("Cargo.toml");
            args.run(Some(&manifest))?;
            let names = if pure_only {
                id_names::typescript_rename_map_partial(&metadata, &crate_root.join("uniffi.toml"))?
            } else {
                id_names::typescript_rename_map(&metadata, &crate_root.join("uniffi.toml"))?
            };
            id_names::rewrite_generated_bindings(out, &names)?;
            let binding = out.join("xmtp_sdk.ts");
            if !pure_only {
                fs::write(
                    &binding,
                    callback_cursor::rewrite(&fs::read_to_string(&binding)?)?,
                )?;
            }
            if is_wasm && !pure_only {
                let mut body = fs::read_to_string(&binding)?;
                body.push_str("\nexport { ConversationID, InboxID, InstallationID, Message, MessageID, Timestamp } from './runtime';\n");
                fs::write(&binding, body)?;
            }
            let index = out.join("index.ts");
            let mut source = fs::read_to_string(&index)?;
            if pure_only {
                source.push_str("\nlet pureLoading: Promise<void> | undefined;\nexport function initPureWasm(wasm: URL = new URL('./xmtp_sdk.wasm', import.meta.url)): Promise<void> { pureLoading ??= uniffiInitAsync(wasm); return pureLoading; }\nexport { TextCodec, MarkdownCodec, ReadReceiptCodec, ReactionV2Codec, AttachmentCodec, RemoteAttachmentCodec, MultiRemoteAttachmentCodec, TransactionReferenceCodec, WalletSendCallsCodec, ActionsCodec, IntentCodec, ReplyCodec, GroupUpdatedCodec, DeleteMessageCodec, LeaveRequestCodec } from './runtime/codecs';\n");
                source.push_str("export { ConversationID, InboxID, InstallationID, MessageID, Timestamp } from './runtime';\n");
            } else if is_wasm {
                source.push_str("\nexport { Client, Message, InboxID, InstallationID, ConversationID, MessageID, Timestamp, MessageStream } from './runtime';\n");
            } else {
                source.push_str("\nexport { Client, Message, InboxID, InstallationID, ConversationID, MessageID, Timestamp, MessageStream, setLogSink, TextCodec, MarkdownCodec, ReadReceiptCodec, ReactionV2Codec, AttachmentCodec, RemoteAttachmentCodec, MultiRemoteAttachmentCodec, TransactionReferenceCodec, WalletSendCallsCodec, ActionsCodec, IntentCodec, ReplyCodec, GroupUpdatedCodec, DeleteMessageCodec, LeaveRequestCodec } from './runtime';\n");
            }
            fs::write(index, source)?;
            if is_wasm && !pure_only {
                bridge::generate(lib, out)?;
            }
            for stale in [".bindgen-manifest", "abi"] {
                let stale_dir = out.join(stale);
                if stale_dir.is_dir() {
                    fs::remove_dir_all(stale_dir)?;
                }
            }
        }
    }

    // The metadata marker validates pure exports. It is not public API text.
    let binding = match language {
        Language::Swift => out.join("xmtp_sdk.swift"),
        Language::Kotlin => out.join("uniffi/xmtp_sdk/xmtp_sdk.kt"),
        Language::TypescriptNapi | Language::TypescriptWasm => out.join("xmtp_sdk.ts"),
    };
    strip_pure_doc_marker(&binding)?;

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
    if pure_only {
        let pure_runtime = out.join("runtime");
        fs::create_dir_all(&pure_runtime)?;
        for name in ["codecs.ts", "codec-type.ts", "ids.ts"] {
            fs::copy(runtime.join(name), pure_runtime.join(name))?;
        }
        fs::copy(runtime.join("pure-index.ts"), pure_runtime.join("index.ts"))?;
    } else {
        copy_tree(runtime.as_std_path(), out.join("runtime").as_std_path())?;
        if matches!(language, Language::TypescriptWasm) {
            fs::copy(
                runtime.join("worker-index.ts"),
                out.join("runtime/index.ts"),
            )?;
            fs::remove_file(out.join("runtime/codecs.ts"))?;
            fs::remove_file(out.join("runtime/logging.ts"))?;
            fs::copy(
                runtime.join("worker-message.ts"),
                out.join("runtime/message.ts"),
            )?;
        }
    }
    if matches!(language, Language::Swift | Language::Kotlin) {
        forwarding::generate(&metadata, language, out)?;
    }
    Ok(())
}

fn validate_pure_module(groups: &MetadataGroupMap) -> Result<()> {
    let items = groups
        .values()
        .flat_map(|group| &group.items)
        .collect::<Vec<_>>();
    validate_pure_items(&items)
}

fn validate_pure_items(items: &[&Metadata]) -> Result<()> {
    let mut pure_count = 0;
    for item in items {
        match item {
            Metadata::Func(function)
                if !function.is_async
                    && function
                        .docstring
                        .as_deref()
                        .is_some_and(|doc| doc.contains("@xmtp-pure")) =>
            {
                pure_count += 1;
            }
            Metadata::Func(function) => bail!("{}: non-pure function in pure WASM", function.name),
            Metadata::Method(method) => {
                bail!("{}.{}: method in pure WASM", method.self_name, method.name)
            }
            Metadata::Constructor(method) => bail!(
                "{}.{}: constructor in pure WASM",
                method.self_name,
                method.name
            ),
            Metadata::TraitMethod(method) => bail!(
                "{}.{}: trait method in pure WASM",
                method.trait_name,
                method.name
            ),
            Metadata::Object(object) => bail!("{}: object in pure WASM", object.name),
            Metadata::CallbackInterface(interface) => {
                bail!("{}: callback in pure WASM", interface.name)
            }
            Metadata::Record(_) | Metadata::Enum(_) | Metadata::CustomType(_) => {}
            _ => bail!("{item:?}: unsupported metadata in pure WASM"),
        }
    }
    if pure_count == 0 {
        bail!("pure WASM contains no pure exports");
    }
    Ok(())
}

fn strip_pure_doc_marker(path: &Utf8Path) -> Result<()> {
    let source = fs::read_to_string(path)?;
    fs::write(path, source.replace("@xmtp-pure", ""))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use uniffi_meta::FnMetadata;

    fn function(name: &str, pure: bool) -> Metadata {
        Metadata::Func(FnMetadata {
            module_path: "test".into(),
            name: name.into(),
            orig_name: None,
            is_async: false,
            inputs: vec![],
            return_type: None,
            throws: None,
            checksum: None,
            docstring: pure.then(|| "@xmtp-pure".into()),
        })
    }

    #[test]
    fn pure_artifact_rejects_any_non_pure_export() {
        let pure = function("encode_standard", true);
        let impure = function("blocking_lookup", false);
        assert!(validate_pure_items(&[&pure]).is_ok());
        assert!(
            validate_pure_items(&[&pure, &impure])
                .unwrap_err()
                .to_string()
                .contains("blocking_lookup")
        );
    }

    #[test]
    fn pure_marker_does_not_reach_generated_docs() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = Utf8Path::from_path(dir.path())
            .context("test directory is not UTF-8")?
            .join("binding.swift");
        fs::write(&path, "/// @xmtp-pure\npublic func encodeText() {}\n")?;
        strip_pure_doc_marker(&path)?;
        assert!(!fs::read_to_string(path)?.contains("@xmtp-pure"));
        Ok(())
    }
}
