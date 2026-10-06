//! Stock UniFFI generation for the standalone migration module.
use super::*;

pub(crate) fn generate(lib: &Utf8Path, language: Language, out: &Utf8Path) -> Result<()> {
    fs::create_dir_all(out)?;
    let config = Utf8Path::new("sdks/migration/dev/uniffi-global.toml");
    match language {
        Language::Swift | Language::Kotlin => {
            bindings::generate(bindings::GenerateOptions {
                languages: vec![match language {
                    Language::Swift => bindings::TargetLanguage::Swift,
                    _ => bindings::TargetLanguage::Kotlin,
                }],
                source: lib.to_owned(),
                out_dir: out.to_owned(),
                config_override: Some(config.to_owned()),
                format: false,
                crate_filter: Some("xmtp_legacy_migration".into()),
                metadata_no_deps: true,
            })?;
        }
        Language::TypescriptNapi | Language::TypescriptWasm => {
            let wasm = matches!(language, Language::TypescriptWasm);
            let scratch = tempfile::tempdir()?;
            let scratch =
                Utf8Path::from_path(scratch.path()).context("temporary path is not UTF-8")?;
            let mut args = BindingsArgs::new(
                SwitchArgs {
                    flavor: if wasm {
                        AbiFlavor::Wasm2
                    } else {
                        AbiFlavor::Napi
                    },
                },
                SourceArgs::library(&lib.to_owned()).with_config(Some(Utf8PathBuf::from(
                    "sdks/migration/dev/typescript.toml",
                ))),
                OutputArgs::new(out, &scratch.join("abi"), true),
            );
            if !wasm {
                args = args.with_lib_resolution(LibResolution::Colocated);
            }
            fs::create_dir_all(scratch.join("src"))?;
            fs::write(
                scratch.join("Cargo.toml"),
                "[package]\nname = \"xmtp-migration-bindgen-input\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[workspace]\n",
            )?;
            fs::write(scratch.join("src/lib.rs"), "")?;
            args.run(Some(&scratch.join("Cargo.toml")))?;
            if wasm {
                ubrn_common::stage_wasm(lib, out, "xmtp_legacy_migration", false)?;
                fs::write(
                    out.join("storage-pool.gen.ts"),
                    format!(
                        "export const storagePoolLock = {:?};\n",
                        format!("xmtp:{}", xmtp_configuration::WASM_VFS_DIRECTORY)
                    ),
                )?;
            }
        }
    }
    Ok(())
}
