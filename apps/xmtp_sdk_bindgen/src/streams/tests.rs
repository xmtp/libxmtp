use super::*;
use crate::{Language, test_metadata};
use uniffi_meta::{FnParamMetadata, ObjectImpl, ObjectMetadata};

fn object(name: &str) -> Metadata {
    Metadata::Object(ObjectMetadata {
        module_path: "xmtp_sdk".into(),
        name: name.into(),
        orig_name: None,
        remote: false,
        imp: ObjectImpl::Struct,
        docstring: None,
    })
}
fn object_type(name: &str) -> Type {
    Type::Object {
        module_path: "xmtp_sdk".into(),
        name: name.into(),
        imp: ObjectImpl::Struct,
    }
}
fn method(receiver: &str, name: &str) -> MethodMetadata {
    MethodMetadata {
        module_path: "xmtp_sdk".into(),
        self_name: receiver.into(),
        name: name.into(),
        orig_name: None,
        is_async: false,
        inputs: vec![],
        return_type: Some(Type::UInt64),
        throws: None,
        takes_self_by_arc: false,
        checksum: None,
        docstring: Some("@xmtp-host-internal @xmtp-internal @xmtp-immutable".into()),
    }
}
fn receiver(name: &str) -> Vec<Metadata> {
    let mut reader = method(name, "selected_reader");
    reader.is_async = true;
    reader.inputs = vec![FnParamMetadata::simple(
        "selection",
        test_metadata::optional(test_metadata::record_type("MessageReaderOptions")),
    )];
    reader.return_type = Some(object_type("MessageReader"));
    reader.throws = Some(test_metadata::enum_type("XmtpError"));
    reader.docstring = Some("@xmtp-stream=consume:MessageStreamOptions:private_owner".into());
    vec![
        object(name),
        Metadata::Method(method(name, "private_owner")),
        Metadata::Method(reader),
    ]
}
fn valid() -> Vec<Metadata> {
    let mut items = receiver("Renamed");
    items.extend([object("MessageReader"), object("ConversationReader")]);
    for name in [
        "MessageReaderOptions",
        "ConversationReaderOptions",
        "ConversationMessageReaderOptions",
    ] {
        items.push(test_metadata::record(name, vec![]));
    }
    items
}
fn named<'a>(items: &'a mut [Metadata], name: &str) -> &'a mut MethodMetadata {
    items
        .iter_mut()
        .find_map(|item| match item {
            Metadata::Method(method) if method.name == name => Some(method),
            _ => None,
        })
        .unwrap()
}
fn checked(items: Vec<Metadata>) -> Result<Vec<Stream>> {
    let groups = test_metadata::groups(items);
    markers::validate(&groups)?;
    resolve(
        &groups
            .values()
            .flat_map(|group| &group.items)
            .collect::<Vec<_>>(),
    )
}
fn reject(items: Vec<Metadata>, expected: &str) {
    let error = format!("{:#}", checked(items).unwrap_err());
    assert!(error.contains(expected), "{error}");
}

#[xmtp_common::test(unwrap_try = true)]
fn renamed_readers_owners_and_fourth_receiver_drive_each_target() {
    let mut items = valid();
    for name in ["Second", "Third", "Fourth"] {
        items.extend(receiver(name));
    }
    let streams = checked(items)?;
    assert_eq!(streams.len(), 4);
    for stream in &streams {
        let ts = typescript_member(stream);
        assert!(ts.contains(&format!("unwrap{}(this)", stream.receiver)));
        assert!(ts.contains("source.privateOwner()"));
        assert!(ts.contains("source.selectedReader(selection, asyncOptions)"));
        let swift = render::native_member(stream, Language::Swift);
        assert!(swift.contains(&format!("public extension {}", stream.receiver)));
        assert!(swift.contains("self.privateOwner()"));
        assert!(swift.contains("self.selectedReader(selection: $0)"));
        let kotlin = render::native_member(stream, Language::Kotlin);
        assert!(kotlin.contains(&format!("fun {}.consume(", stream.receiver)));
        assert!(kotlin.contains("privateOwner(), { selectedReader(it) }"));
    }
    let imports = typescript_import(&streams, false);
    assert!(imports.contains("openMessageStreamOptions"));
    assert!(!imports.contains("ConversationStreamOptions"));
    assert!(typescript_import(&streams, true).is_empty());
    let exports = typescript_exports(&streams);
    assert!(exports.contains("MessageStream"));
    assert!(!exports.contains("ConversationStream"));
}

#[xmtp_common::test(unwrap_try = true)]
fn invalid_reader_shapes_and_missing_records_stop_generation() {
    for (change, expected) in [
        (0, "async fallible"),
        (1, "one input"),
        (2, "optional record"),
        (3, "XmtpError"),
        (4, "MessageReader or ConversationReader"),
        (5, "does not match"),
        (6, "input record"),
        (7, "stream result"),
    ] {
        let mut items = valid();
        let reader = named(&mut items, "selected_reader");
        match change {
            0 => reader.is_async = false,
            1 => reader.inputs.clear(),
            2 => reader.inputs[0].ty = test_metadata::optional(Type::String),
            3 => reader.throws = Some(test_metadata::enum_type("OtherError")),
            4 => reader.return_type = Some(object_type("EventReader")),
            5 => reader.docstring = Some("@xmtp-stream=consume:ConversationStreamOptions:private_owner".into()),
            6 => items.retain(|item| !matches!(item, Metadata::Record(record) if record.name == "MessageReaderOptions")),
            7 => items.retain(|item| !matches!(item, Metadata::Object(object) if object.name == "MessageReader")),
            _ => unreachable!(),
        }
        reject(items, expected);
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn invalid_owner_references_stop_generation() {
    for change in 0..6 {
        let mut items = valid();
        let owner = named(&mut items, "private_owner");
        match change {
            0 => owner.name = "different_owner".into(),
            1 => owner.docstring = Some("@xmtp-immutable".into()),
            2 => owner.docstring = Some("@xmtp-host-internal @xmtp-internal".into()),
            3 => owner.return_type = Some(Type::String),
            4 => owner.is_async = true,
            5 => owner
                .inputs
                .push(FnParamMetadata::simple("argument", Type::UInt64)),
            _ => unreachable!(),
        }
        reject(
            items,
            "needs one immutable, host-internal, argument-free u64 method",
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn stream_markers_names_and_host_types_reject_collisions() {
    for (marker, expected) in [
        (
            "@xmtp-stream=bad.code:MessageStreamOptions:private_owner",
            "ASCII identifiers",
        ),
        (
            "@xmtp-stream=consume:OtherOptions:private_owner",
            "unsupported stream options",
        ),
        (
            "@xmtp-stream=consume:MessageStreamOptions:private_owner @xmtp-stream=other:MessageStreamOptions:private_owner",
            "needs one object reader method",
        ),
        (
            "@xmtp-stream=consume:MessageStreamOptions:private_owner @xmtp-internal",
            "reader must remain public",
        ),
        (
            "@xmtp-stream=constructor:MessageStreamOptions:private_owner",
            "reserved public stream name",
        ),
    ] {
        let mut items = valid();
        named(&mut items, "selected_reader").docstring = Some(marker.into());
        reject(items, expected);
    }
    let mut items = valid();
    let mut duplicate = named(&mut items, "selected_reader").clone();
    duplicate.name = "another_reader".into();
    items.push(Metadata::Method(duplicate));
    reject(items, "duplicate or reserved public stream name");
    let mut items = valid();
    items.push(Metadata::Method(method("Renamed", "consume")));
    reject(items, "collides with an exported method");
    let mut items = valid();
    items.push(test_metadata::record("MessageStreamOptions", vec![]));
    reject(items, "host stream type collides");
    let mut items = valid();
    let mut record = test_metadata::record("Other", vec![]);
    let Metadata::Record(value) = &mut record else {
        unreachable!()
    };
    value.docstring = Some("@xmtp-stream=consume:MessageStreamOptions:private_owner".into());
    items.push(record);
    reject(items, "needs one object reader method");
}

#[xmtp_common::test(unwrap_try = true)]
fn common_forwarding_uses_declared_names_and_keeps_extension_locations() {
    let mut items = valid();
    items.extend(receiver("Group"));
    items.extend(receiver("Dm"));
    let streams = checked(items.clone())?;
    for language in [Language::Swift, Language::Kotlin] {
        let text = crate::forwarding::stream_methods(&streams, language)?;
        assert!(text.contains("group.consume("));
        assert!(text.contains("dm.consume("));
        assert!(!text.contains("streamMessages"));
        let dir = tempfile::tempdir()?;
        let out = camino::Utf8Path::from_path(dir.path())?;
        let extension = if matches!(language, Language::Swift) {
            "swift"
        } else {
            "kt"
        };
        std::fs::create_dir_all(out.join("runtime/streams"))?;
        let path = out.join(format!("runtime/streams/StreamMethods.{extension}"));
        std::fs::write(&path, "// Runtime options and typed helpers.\n")?;
        generate_native(&streams, language, out)?;
        let source = std::fs::read_to_string(path)?;
        assert!(source.starts_with("// Runtime options and typed helpers."));
        assert!(source.contains(&text));
        assert!(source.contains("privateOwner()"));
    }
    for item in &mut items {
        if let Metadata::Method(method) = item
            && method.self_name == "Dm"
            && method.name == "selected_reader"
        {
            method.docstring =
                Some("@xmtp-stream=consume:ConversationMessageStreamOptions:private_owner".into());
            method.inputs[0].ty = test_metadata::optional(test_metadata::record_type(
                "ConversationMessageReaderOptions",
            ));
        }
    }
    reject(items, "conflicting common stream signatures");
}
