//! Private native conformance metadata must not become browser operations.
use super::*;
use uniffi_meta::{FnParamMetadata, MethodMetadata, RecordMetadata};

const SOURCE: &str = include_str!("../../../../crates/xmtp_sdk/src/client/event_conformance.rs");

fn docs_before(source: &str, declaration: &str) -> String {
    source
        .split_once(declaration)
        .expect("the native conformance declaration")
        .0
        .lines()
        .rev()
        .take_while(|line| line.trim_start().starts_with("///"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn methods() -> Vec<Metadata> {
    [
        (
            "sdk_conformance_emit_hmac_events",
            Type::UInt32,
            Some(Type::String),
        ),
        (
            "sdk_conformance_listener_counts",
            Type::Record {
                module_path: "xmtp_sdk".into(),
                name: "SdkConformanceListenerCounts".into(),
            },
            None,
        ),
    ]
    .into_iter()
    .map(|(name, output, throws)| {
        Metadata::Method(MethodMetadata {
            module_path: "xmtp_sdk".into(),
            self_name: "Client".into(),
            name: name.into(),
            orig_name: None,
            is_async: false,
            inputs: vec![FnParamMetadata::simple("value", Type::UInt32)],
            return_type: Some(output),
            throws,
            takes_self_by_arc: false,
            checksum: None,
            docstring: Some(docs_before(SOURCE, &format!("    pub fn {name}"))),
        })
    })
    .collect()
}

fn counts() -> Metadata {
    Metadata::Record(RecordMetadata {
        module_path: "xmtp_sdk".into(),
        name: "SdkConformanceListenerCounts".into(),
        orig_name: None,
        remote: false,
        fields: vec![],
        docstring: Some(docs_before(SOURCE, "#[derive(uniffi::Record)]")),
    })
}

#[xmtp_common::test(unwrap_try = true)]
fn native_conformance_methods_stay_private_and_out_of_bridge() {
    let mut items = methods();
    items.push(counts());
    validate_bridge(&items)?;
    let operations = operations(&items);
    assert!(
        operations.is_empty(),
        "a mutable native helper became a proxy operation"
    );
    let files = render(&items, &operations, "test")?;
    for file in ["proxy.gen.ts", "dispatch.gen.ts", "contract.gen.ts"] {
        assert!(!files[file].contains("sdkConformanceEmitHmacEvents"));
        assert!(!files[file].contains("sdkConformanceListenerCounts"));
    }
    let refs = items.iter().collect::<Vec<_>>();
    let members = crate::public_projection::client_members_for_test(&refs)?;
    assert!(!members.contains("sdkConformance"));
    for target in [
        crate::public_projection::Target::Node,
        crate::public_projection::Target::Browser,
    ] {
        let api = crate::public_projection::public_api_for_test(&refs, target);
        assert!(!api.contains("SdkConformanceListenerCounts"));
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn private_sync_method_requires_worker_marker() {
    for mut item in methods() {
        let Metadata::Method(method) = &mut item else {
            unreachable!()
        };
        let name = method.name.clone();
        method.docstring = method
            .docstring
            .as_ref()
            .map(|doc| doc.replace("@xmtp-worker", ""));
        let error = validate_bridge(&[item]).unwrap_err();
        assert!(error.to_string().contains(&format!("Client.{name}")));
    }
}
