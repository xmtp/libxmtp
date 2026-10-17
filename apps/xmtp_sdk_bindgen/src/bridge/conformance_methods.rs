//! Private worker-only `Client` methods and records must not become browser
//! operations. The metadata is a fixture: no SDK export has this shape now.
use super::*;
use uniffi_meta::{FnParamMetadata, MethodMetadata, ObjectMetadata, RecordMetadata};

fn methods() -> Vec<Metadata> {
    [
        (
            "sdk_conformance_emit_hmac_events",
            Type::UInt32,
            Some(Type::String),
            "Fill the native conformance queue. @xmtp-worker @xmtp-internal",
        ),
        (
            "sdk_conformance_listener_counts",
            Type::Record {
                module_path: "xmtp_sdk".into(),
                name: "SdkConformanceListenerCounts".into(),
            },
            None,
            "Read the native conformance queue. @xmtp-worker @xmtp-internal",
        ),
    ]
    .into_iter()
    .map(|(name, output, throws, docs)| {
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
            docstring: Some(docs.into()),
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
        docstring: Some("Private listener counts. @xmtp-internal".into()),
    })
}

#[xmtp_common::test(unwrap_try = true)]
fn native_conformance_methods_stay_private_and_out_of_bridge() {
    let mut items = methods();
    items.push(counts());
    items.push(Metadata::Object(ObjectMetadata {
        module_path: "xmtp_sdk".into(),
        name: "Client".into(),
        orig_name: None,
        remote: false,
        imp: ObjectImpl::Struct,
        docstring: None,
    }));
    validate_bridge(&items)?;
    let operations = operations(&items);
    assert!(
        operations.is_empty(),
        "a mutable native helper became a proxy operation"
    );
    let files = render(&items, &operations, "test")?;
    for file in ["proxy.gen.ts", "dispatch.gen.ts", "contract.gen.ts"] {
        assert!(!files[file].contains("sdkConformanceEmitHmacEvents("));
        assert!(!files[file].contains("sdkConformanceListenerCounts("));
    }
    assert!(files["proxy.gen.ts"].contains("implements Omit<B.ClientLike, \"sdkConformanceEmitHmacEvents\" | \"sdkConformanceListenerCounts\">"));
    assert!(files["public-client.gen.ts"].contains("protected binding(): ClientBinding"));
    let refs = items.iter().collect::<Vec<_>>();
    let forwarding = crate::forwarding::typescript_forwarders(&refs);
    assert!(forwarding.contains("export type ClientBinding = Omit<ClientLike,"));
    assert!(!forwarding.contains("sdkConformanceEmitHmacEvents("));
    assert!(!forwarding.contains("sdkConformanceListenerCounts("));
    let members = crate::public_projection::client_members_for_test(&refs)?;
    assert!(!members.contains("sdkConformance"));
    assert!(members.contains("WeakMap<ClientMembers, ClientBinding>"));
    assert!(!members.contains("B.ClientLike"));
    // The strict type control consumes these actual renderer outputs.
    if let Some(out) = std::env::var_os("SDK_BINDGEN_TYPED_CONTROL_OUT") {
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out)?;
        for name in ["proxy.gen.ts", "public-client.gen.ts"] {
            std::fs::write(out.join(name), &files[name])?;
        }
        std::fs::write(out.join("client-forwarding.gen.ts"), &forwarding)?;
        std::fs::write(out.join("client-members.gen.ts"), &members)?;
    }
    for target in [
        crate::public_projection::Target::Node,
        crate::public_projection::Target::Browser,
    ] {
        let api = crate::public_projection::public_api_for_test(&refs, target);
        assert!(!api.contains("SdkConformanceListenerCounts"));
        assert!(!api.contains("ClientBinding"));
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
