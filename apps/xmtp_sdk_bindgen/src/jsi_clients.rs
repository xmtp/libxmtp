//! Retain completed native Clients before any JavaScript promise continuation.
use anyhow::{Context, Result, bail};
use uniffi_meta::{Metadata, MetadataGroupMap, Type};

const COMPLETE: &str = "ffi_xmtp_sdk_rust_future_complete_u64";
const FREE: &str = "ffi_xmtp_sdk_rust_future_free_u64";

pub(crate) fn rewrite(source: &str, metadata: &MetadataGroupMap) -> Result<String> {
    let mut source = source.to_owned();
    let mut clients = 0;
    for item in metadata.values().flat_map(|group| &group.items) {
        let call = match item {
            Metadata::Constructor(call) if call.self_name == "Client" => {
                Some((call.ffi_symbol_name(), call.is_async))
            }
            Metadata::Func(call) if returns_client(call.return_type.as_ref()) => {
                Some((call.ffi_symbol_name(), call.is_async))
            }
            Metadata::Method(call) if returns_client(call.return_type.as_ref()) => {
                Some((call.ffi_symbol_name(), call.is_async))
            }
            _ => None,
        };
        let Some((symbol, is_async)) = call else {
            continue;
        };
        if !is_async {
            bail!("JSI Client ownership requires an async native return: {symbol}");
        }
        change_body(&mut source, &symbol, |body| {
            let marker = format!("// UBRN_NATIVE_FUTURE {symbol}");
            replace_once(
                body,
                &marker,
                &format!(
                    "{marker}\n        xmtp::reactnative::Runtime::clientFutureStarted(callInvoker, value);"
                ),
            )
        })?;
        clients += 1;
    }
    if clients == 0 {
        bail!("JSI metadata has no native Client constructors");
    }
    change_body(&mut source, COMPLETE, |body| {
        let marker = "        uniffi::xmtp_sdk::Bridging<RustCallStatus>::copyIntoJs";
        replace_once(
            body,
            marker,
            &format!(
                "        xmtp::reactnative::Runtime::clientFutureCompleted(callInvoker, uniffi_jsi::Bridging<uint64_t>::fromJs(rt, callInvoker, args[0]), value, status.code);\n{marker}"
            ),
        )
    })?;
    change_body(&mut source, FREE, |body| {
        let marker = "    generation->retireFuture(handle);";
        replace_once(
            body,
            marker,
            &format!(
                "    xmtp::reactnative::Runtime::clientFutureFreed(callInvoker, handle);\n{marker}"
            ),
        )
    })?;
    let discard = "static void discard_object_Client(uint64_t pointer) {";
    if source.matches(discard).count() != 1 {
        bail!("JSI Client ownership expected one native discard helper");
    }
    let start = source
        .find(discard)
        .context("checked Client discard helper")?;
    let end = source[start..]
        .find("\n}")
        .context("Client discard helper has no closing brace")?
        + start;
    if !source[start..end].contains("// UBRN_DISCARD_OBJECT Client") {
        bail!("JSI Client discard template changed");
    }
    source.replace_range(
        start..end,
        &format!(
            "{discard}\n // UBRN_DISCARD_OBJECT Client\n xmtp::reactnative::closeClient(pointer);"
        ),
    );
    source.insert_str(0, "#include \"../XmtpRuntime.h\"\n");
    Ok(source)
}

fn returns_client(ty: Option<&Type>) -> bool {
    matches!(ty, Some(Type::Object { name, .. }) if name == "Client")
}

fn change_body(
    source: &mut String,
    symbol: &str,
    change: impl FnOnce(&str) -> Result<String>,
) -> Result<()> {
    let signature = format!("jsi::Value NativeXmtpSdk::cpp_{symbol}(");
    if source.matches(&signature).count() != 1 {
        bail!("JSI Client ownership expected one wrapper for {symbol}");
    }
    let start = source.find(&signature).context("checked signature")?;
    let end = source[start..]
        .find("\n}")
        .context("JSI wrapper has no closing brace")?
        + start;
    let body = change(&source[start..end])?;
    source.replace_range(start..end, &body);
    Ok(())
}

fn replace_once(body: &str, marker: &str, replacement: &str) -> Result<String> {
    if body.matches(marker).count() != 1 {
        bail!("JSI Client ownership template changed: {marker}");
    }
    Ok(body.replace(marker, replacement))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_metadata::groups;
    use uniffi_meta::ConstructorMetadata;

    fn constructor(name: &str) -> Metadata {
        Metadata::Constructor(ConstructorMetadata {
            module_path: "xmtp_sdk".into(),
            self_name: "Client".into(),
            name: name.into(),
            orig_name: None,
            is_async: true,
            inputs: vec![],
            throws: None,
            checksum: None,
            docstring: None,
        })
    }

    fn wrapper(symbol: &str, body: &str) -> String {
        format!("jsi::Value NativeXmtpSdk::cpp_{symbol}(args) {{\n{body}\n}}\n")
    }

    fn fixture(names: &[&str]) -> String {
        let mut source = String::new();
        for name in names {
            source.push_str(&wrapper(&format!("uniffi_xmtp_sdk_fn_constructor_client_{name}"),
                &format!("        auto value = native_constructor();\n        // UBRN_NATIVE_FUTURE uniffi_xmtp_sdk_fn_constructor_client_{name}\n        return lift(value);")));
        }
        source.push_str(&wrapper(COMPLETE, "        auto value = native_complete();\n        uniffi::xmtp_sdk::Bridging<RustCallStatus>::copyIntoJs(rt, callInvoker, status, args[count - 1]);\n        return lift(value);"));
        source.push_str(&wrapper(FREE, "    auto handle = native_handle();\n    generation->retireFuture(handle);\n    return undefined;"));
        source.push_str("static void discard_object_Client(uint64_t pointer) {\n // UBRN_DISCARD_OBJECT Client\n generic_free(pointer);\n}\n");
        source
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn native_client_ownership_follows_metadata_and_precedes_js_lifting() {
        let metadata = groups(vec![constructor("create"), constructor("from_backup")]);
        let source = fixture(&["create", "from_backup"]);
        let output = rewrite(&source, &metadata)?;
        assert_eq!(output.matches("Runtime::clientFutureStarted").count(), 2);
        let completed = output.find("Runtime::clientFutureCompleted").unwrap();
        let status = output.find("Bridging<RustCallStatus>::copyIntoJs").unwrap();
        assert!(completed < status);
        assert!(output.contains("Runtime::clientFutureFreed(callInvoker, handle)"));
        assert!(output.contains("xmtp::reactnative::closeClient(pointer)"));
        assert!(!output.contains("generic_free(pointer)"));
        assert!(
            rewrite(
                &source.replace("UBRN_DISCARD_OBJECT Client", "changed"),
                &metadata
            )
            .is_err()
        );
        assert!(rewrite(&fixture(&["create"]), &metadata).is_err());
        assert!(rewrite(&source.replace("UBRN_NATIVE_FUTURE", "changed"), &metadata).is_err());
        assert!(
            rewrite(
                &source.replace(
                    "cpp_ffi_xmtp_sdk_rust_future_complete_u64",
                    "changed_complete"
                ),
                &metadata
            )
            .is_err()
        );
    }
}
