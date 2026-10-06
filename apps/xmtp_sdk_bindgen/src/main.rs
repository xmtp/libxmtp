mod bridge;
mod callback_cursor;
mod callback_results;
mod client_statics;
mod format;
mod forwarding;
mod kotlin_callbacks;
mod kotlin_records;
mod logging_admission;
mod markers;
mod native_visibility;
mod public_projection;
mod redaction;
mod swift_async;
mod swift_events;
mod swift_records;
#[cfg(test)]
mod test_metadata;
mod validate;

use std::{collections::BTreeSet, fs, path::Path};

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
        /// Leave generated TypeScript unformatted. For jobs that only load the
        /// generated package and have no JavaScript toolchain.
        #[arg(long)]
        no_format: bool,
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
            no_format,
        } => {
            if no_format {
                format::disable();
            }
            generate(&lib, language, &out, config.as_deref(), pure_only)
        }
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
    markers::validate(&metadata)?;
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
            strip_doc_markers(&out.join(match language {
                Language::Swift => "xmtp_sdk.swift",
                _ => "uniffi/xmtp_sdk/xmtp_sdk.kt",
            }))?;
            if matches!(language, Language::Swift) {
                let binding = out.join("xmtp_sdk.swift");
                fs::write(
                    &binding,
                    swift_events::rewrite(&swift_records::rewrite(&swift_async::rewrite(
                        &fs::read_to_string(&binding)?,
                    )?)?)?,
                )?;
            }
            if matches!(language, Language::Kotlin) {
                let binding = out.join("uniffi/xmtp_sdk/xmtp_sdk.kt");
                let callbacks = kotlin_callbacks::rewrite(&fs::read_to_string(&binding)?)?;
                let callbacks = logging_admission::kotlin(&callbacks)?;
                let callbacks = native_visibility::kotlin(&callbacks)?;
                fs::write(&binding, kotlin_records::rewrite(&callbacks, &metadata)?)?;
            } else {
                let binding = out.join("xmtp_sdk.swift");
                fs::write(
                    &binding,
                    format::swift_trailing_whitespace(&native_visibility::swift(
                        &logging_admission::swift(&fs::read_to_string(&binding)?)?,
                    )?),
                )?;
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
            let binding = out.join("xmtp_sdk.ts");
            strip_doc_markers(&binding)?;
            if !pure_only {
                fs::write(
                    &binding,
                    callback_results::rewrite(&callback_cursor::rewrite(&fs::read_to_string(
                        &binding,
                    )?)?)?,
                )?;
            }
            if is_wasm && !pure_only {
                let mut body = fs::read_to_string(&binding)?;
                body.push_str("\nexport { Message, Timestamp } from './runtime';\n");
                fs::write(&binding, body)?;
            }
            let index = out.join("index.ts");
            let mut source = fs::read_to_string(&index)?;
            if pure_only {
                source.push_str("\nlet pureLoading: Promise<void> | undefined;\nexport function initPureWasm(wasm: URL = new URL('./xmtp_sdk.wasm', import.meta.url)): Promise<void> { pureLoading ??= uniffiInitAsync(wasm); return pureLoading; }\nexport { TextCodec, MarkdownCodec, ReadReceiptCodec, ReactionV2Codec, AttachmentCodec, RemoteAttachmentCodec, MultiRemoteAttachmentCodec, TransactionReferenceCodec, WalletSendCallsCodec, ActionsCodec, IntentCodec, ReplyCodec, GroupUpdatedCodec, DeleteMessageCodec, LeaveRequestCodec } from './runtime/codecs';\n");
                source.push_str("export { Timestamp } from './runtime';\n");
            } else if is_wasm {
                source.push_str("\nexport { Client, Storage } from './public-client.gen';\nexport type { StorageAdmin } from './storage-admin.gen';\nexport { Message } from './host-message.gen';\nexport { Timestamp, MessageStream, ConversationStream, EventStream } from './runtime';\n");
                source.push_str(
                    "export type { StreamCloseReason, StreamOptions } from './runtime';\n",
                );
            } else {
                source.push_str("\nexport { Client, Message, Timestamp, MessageStream, ConversationStream, EventStream, setLogSink, TextCodec, MarkdownCodec, ReadReceiptCodec, ReactionV2Codec, AttachmentCodec, RemoteAttachmentCodec, MultiRemoteAttachmentCodec, TransactionReferenceCodec, WalletSendCallsCodec, ActionsCodec, IntentCodec, ReplyCodec, GroupUpdatedCodec, DeleteMessageCodec, LeaveRequestCodec } from './runtime';\n");
                source.push_str(
                    "export type { StreamCloseReason, StreamOptions } from './runtime';\n",
                );
            }
            // The Node root names each binding export instead of a star
            // re-export, so internal names such as `sdkLogSinkHandoff` stay
            // out of it.
            if matches!(language, Language::TypescriptNapi) {
                let exports = public_node_exports(&fs::read_to_string(&binding)?, &source);
                source = source.replace("export * from './xmtp_sdk';", &exports);
            }
            // The stock root loads and exports the binding. It stays private as
            // `binding.ts`; the public projection writes the package root.
            fs::write(out.join("binding.ts"), source)?;
            fs::remove_file(index)?;
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
        // The public codecs wrap the pure codecs over public values.
        let pure_public = pure_runtime.join("public");
        fs::create_dir_all(&pure_public)?;
        for name in ["codecs.ts", "codec.ts"] {
            fs::copy(runtime.join("public").join(name), pure_public.join(name))?;
        }
    } else {
        copy_tree(runtime.as_std_path(), out.join("runtime").as_std_path())?;
        if matches!(language, Language::Swift) {
            // Swift structs print every field, so a record with a redacted
            // field gets its description from metadata.
            fs::write(
                out.join("runtime/RecordDescriptions.swift"),
                redaction::swift(&metadata)?,
            )?;
        }
        if matches!(language, Language::Kotlin) {
            let android = runtime
                .parent()
                .context("Kotlin runtime has no parent directory")?
                .join("android");
            copy_tree(android.as_std_path(), out.join("android").as_std_path())?;
        }
        if matches!(language, Language::TypescriptWasm) {
            fs::copy(
                runtime.join("worker-index.ts"),
                out.join("runtime/index.ts"),
            )?;
            fs::remove_file(out.join("runtime/codecs.ts"))?;
            fs::write(
                out.join("runtime/logging.ts"),
                include_str!("../templates/bridge/logging.ts"),
            )?;
            fs::copy(
                runtime.join("worker-message.ts"),
                out.join("runtime/message.ts"),
            )?;
            // The browser package has one Timestamp class: its pure module's.
            fs::write(
                out.join("runtime/ids.ts"),
                "// The browser package shares one Timestamp class with its pure module.\nexport { Timestamp } from \"../../typescript-pure/runtime/ids.js\";\n",
            )?;
        }
    }
    if !pure_only
        && matches!(
            language,
            Language::TypescriptNapi | Language::TypescriptWasm
        )
    {
        forwarding::generate_typescript(&metadata, out)?;
        let target = if matches!(language, Language::TypescriptNapi) {
            public_projection::Target::Node
        } else {
            // The browser target module uses the worker proxies. The browser
            // uses the process log sink through the worker bridge.
            let public = out.join("runtime/public");
            fs::write(
                public.join("host.ts"),
                include_str!("../templates/bridge/public-host.ts"),
            )?;
            fs::remove_file(public.join("codecs.ts"))?;
            public_projection::Target::Browser
        };
        public_projection::generate(&metadata, out, target)?;
    }
    if pure_only {
        public_projection::generate(&metadata, out, public_projection::Target::Pure)?;
    }
    if matches!(language, Language::Swift | Language::Kotlin) {
        forwarding::generate(&metadata, language, out)?;
    }
    if matches!(language, Language::Swift) {
        // Forwarding adds documentation after the initial binding rewrite.
        let binding = out.join("xmtp_sdk.swift");
        fs::write(
            &binding,
            format::swift_trailing_whitespace(&fs::read_to_string(&binding)?),
        )?;
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
                    && markers::has(function.docstring.as_deref(), markers::PURE) =>
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

/// Metadata markers describe items to this generator; they are not public API
/// text. They go before any rewrite reads the stock binding, so a declaration
/// that UniFFI writes after a comment keeps the spacing it had before it.
fn strip_doc_markers(path: &Utf8Path) -> Result<()> {
    let source = fs::read_to_string(path)?;
    fs::write(path, markers::strip(&source))?;
    Ok(())
}

fn public_node_exports(binding: &str, index: &str) -> String {
    let mut values = BTreeSet::new();
    let mut types = BTreeSet::new();
    let mut overrides = BTreeSet::new();
    for line in index.lines() {
        if let Some(rest) = line.strip_prefix("export { ")
            && let Some(list) = rest.strip_suffix(" } from './runtime';")
        {
            overrides.extend(list.split(", "));
        }
    }
    for line in binding.lines() {
        let mut words = line.split_whitespace();
        if words.next() != Some("export") {
            continue;
        }
        let kind = words.next();
        let name = if kind == Some("async") {
            if words.next() == Some("function") {
                words.next()
            } else {
                None
            }
        } else if matches!(
            kind,
            Some("class" | "const" | "enum" | "function" | "interface" | "type")
        ) {
            words.next()
        } else {
            None
        };
        if let Some(name) = name {
            let name = name
                .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .next()
                .unwrap_or("");
            if name != "sdkLogSinkHandoff" && !overrides.contains(name) {
                if matches!(kind, Some("interface" | "type")) {
                    types.insert(name.to_owned());
                } else {
                    values.insert(name.to_owned());
                }
            }
        }
    }
    types.retain(|name| !values.contains(name));
    format!(
        "export {{ {} }} from './xmtp_sdk';\nexport type {{ {} }} from './xmtp_sdk';",
        values.into_iter().collect::<Vec<_>>().join(", "),
        types.into_iter().collect::<Vec<_>>().join(", ")
    )
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

    #[xmtp_common::test(unwrap_try = true)]
    fn markers_do_not_reach_generated_docs() {
        let dir = tempfile::tempdir()?;
        let path = Utf8Path::from_path(dir.path())
            .context("test directory is not UTF-8")?
            .join("binding.swift");
        fs::write(
            &path,
            "/**\n * @xmtp-pure\n */\npublic func encodeText() {}\n/**\n * The ID.\n * @xmtp-immutable\n */\nfunc id() {}\n",
        )?;
        strip_doc_markers(&path)?;
        assert_eq!(
            fs::read_to_string(path)?,
            "public func encodeText() {}\n/**\n * The ID.\n */\nfunc id() {}\n"
        );
    }

    #[test]
    fn log_admission_is_internal_to_node_runtime() {
        let exports = public_node_exports(
            "export function sdkLogSinkHandoff() {}\nexport async function create() {}\nexport type Entry = string;",
            "",
        );
        assert_eq!(
            exports,
            "export { create } from './xmtp_sdk';\nexport type { Entry } from './xmtp_sdk';"
        );
    }
}
