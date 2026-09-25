use anyhow::{Result, bail};
use std::collections::HashSet;
use uniffi_meta::{Metadata, MetadataGroupMap, Type};

pub(crate) fn validate_metadata(groups: &MetadataGroupMap) -> Result<()> {
    for group in groups.values() {
        validate_items(group.items.iter())?;
    }
    Ok(())
}

fn validate_items<'a>(items: impl IntoIterator<Item = &'a Metadata>) -> Result<()> {
    let items: Vec<&Metadata> = items.into_iter().collect();
    let records: HashSet<&str> = items
        .iter()
        .filter_map(|item| match item {
            Metadata::Record(record) => Some(record.name.as_str()),
            _ => None,
        })
        .collect();
    let foreign_traits: HashSet<&str> = items
        .iter()
        .filter_map(|item| match item {
            Metadata::Object(object) if object.imp.has_callback_interface() => {
                Some(object.name.as_str())
            }
            Metadata::CallbackInterface(interface) => Some(interface.name.as_str()),
            _ => None,
        })
        .collect();

    for item in &items {
        match item {
            Metadata::Record(record) => {
                for field in &record.fields {
                    if let Type::Optional { inner_type } = &field.ty
                        && type_names_record(inner_type, &record.name)
                    {
                        bail!("{}.{}: self-typed optional field", record.name, field.name);
                    }
                    if matches!(
                        record.name.as_str(),
                        "MessageData" | "ReactionMessage" | "ReplyParent"
                    ) && contains_object(&field.ty, &items, &mut HashSet::new())
                    {
                        bail!(
                            "{}.{}: message record contains an object",
                            record.name,
                            field.name
                        );
                    }
                }
            }
            Metadata::Object(object) if object.name.ends_with("Reader") => {
                let has_end = items.iter().any(|item| match item {
                    Metadata::Method(method) => {
                        method.self_name == object.name && method.name == "end" && method.is_async
                    }
                    Metadata::TraitMethod(method) => {
                        method.trait_name == object.name && method.name == "end" && method.is_async
                    }
                    _ => false,
                });
                if !has_end {
                    bail!("{}: exported Reader has no async end method", object.name);
                }
            }
            Metadata::Method(method) => {
                let item_name = format!("{}.{}", method.self_name, method.name);
                check_message_inputs(&item_name, &method.inputs)?;
                if records.contains(method.self_name.as_str()) {
                    bail!("{item_name}: exported record method is not supported");
                }
                if method.name == "close" {
                    bail!("{item_name}: exported object close method is not supported");
                }
                if foreign_traits.contains(method.self_name.as_str())
                    && !method.is_async
                    && item_name != "LogSink.log"
                {
                    bail!("{item_name}: synchronous foreign-trait method is not supported");
                }
                check_error_type(&item_name, method.throws.as_ref())?;
            }
            Metadata::TraitMethod(method) => {
                let item_name = format!("{}.{}", method.trait_name, method.name);
                check_message_inputs(&item_name, &method.inputs)?;
                if method.name == "close" {
                    bail!("{item_name}: exported object close method is not supported");
                }
                if foreign_traits.contains(method.trait_name.as_str())
                    && !method.is_async
                    && item_name != "LogSink.log"
                {
                    bail!("{item_name}: synchronous foreign-trait method is not supported");
                }
                check_error_type(&item_name, method.throws.as_ref())?;
            }
            Metadata::Constructor(constructor) => {
                check_message_inputs(
                    &format!("{}.{}", constructor.self_name, constructor.name),
                    &constructor.inputs,
                )?;
                check_error_type(
                    &format!("{}.{}", constructor.self_name, constructor.name),
                    constructor.throws.as_ref(),
                )?;
            }
            Metadata::Func(function) => {
                check_message_inputs(&function.name, &function.inputs)?;
                check_error_type(&function.name, function.throws.as_ref())?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn type_names_record(ty: &Type, name: &str) -> bool {
    match ty {
        Type::Record { name: found, .. } => found == name,
        Type::Box { inner_type } => type_names_record(inner_type, name),
        _ => false,
    }
}

fn contains_object(
    ty: &Type,
    items: &[&Metadata],
    visited: &mut HashSet<(String, String)>,
) -> bool {
    match ty {
        Type::Object { .. } => true,
        Type::Optional { inner_type }
        | Type::Sequence { inner_type }
        | Type::Set { inner_type }
        | Type::Box { inner_type } => contains_object(inner_type, items, visited),
        Type::Map {
            key_type,
            value_type,
        } => {
            contains_object(key_type, items, visited) || contains_object(value_type, items, visited)
        }
        Type::Custom { builtin, .. } => contains_object(builtin, items, visited),
        Type::Record { module_path, name } | Type::Enum { module_path, name } => {
            if !visited.insert((module_path.clone(), name.clone())) {
                return false;
            }
            items.iter().any(|item| match item {
                Metadata::Record(record)
                    if record.module_path == *module_path && record.name == *name =>
                {
                    record
                        .fields
                        .iter()
                        .any(|field| contains_object(&field.ty, items, visited))
                }
                Metadata::Enum(enumeration)
                    if enumeration.module_path == *module_path && enumeration.name == *name =>
                {
                    enumeration.variants.iter().any(|variant| {
                        variant
                            .fields
                            .iter()
                            .any(|field| contains_object(&field.ty, items, visited))
                    })
                }
                _ => false,
            })
        }
        _ => false,
    }
}

fn takes_message_record(ty: &Type) -> bool {
    match ty {
        Type::Record { name, .. }
            if matches!(
                name.as_str(),
                "MessageData" | "ReactionMessage" | "ReplyParent"
            ) =>
        {
            true
        }
        Type::Custom { name, .. } if name == "Message" => true,
        Type::Optional { inner_type }
        | Type::Sequence { inner_type }
        | Type::Box { inner_type } => takes_message_record(inner_type),
        Type::Map {
            key_type,
            value_type,
        } => takes_message_record(key_type) || takes_message_record(value_type),
        _ => false,
    }
}

fn check_message_inputs(item_name: &str, inputs: &[uniffi_meta::FnParamMetadata]) -> Result<()> {
    if inputs.iter().any(|input| takes_message_record(&input.ty)) {
        bail!("{item_name}: exported function takes a message record; pass MessageID");
    }
    Ok(())
}

fn check_error_type(item_name: &str, thrown: Option<&Type>) -> Result<()> {
    if let Some(Type::Object { name, .. }) = thrown {
        bail!("{item_name}: object {name} cannot be used as an error");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uniffi_meta::{
        CallbackInterfaceMetadata, EnumMetadata, EnumShape, FieldMetadata, FnMetadata,
        FnParamMetadata, MethodMetadata, ObjectImpl, ObjectMetadata, RecordMetadata, TraitKind,
        TraitMethodMetadata, VariantMetadata,
    };

    fn object(name: &str, imp: ObjectImpl) -> Metadata {
        Metadata::Object(ObjectMetadata {
            module_path: "test".into(),
            name: name.into(),
            orig_name: None,
            remote: false,
            imp,
            docstring: None,
        })
    }

    fn method(owner: &str, name: &str, is_async: bool) -> Metadata {
        Metadata::Method(MethodMetadata {
            module_path: "test".into(),
            self_name: owner.into(),
            name: name.into(),
            orig_name: None,
            is_async,
            inputs: vec![],
            return_type: None,
            throws: None,
            takes_self_by_arc: true,
            checksum: None,
            docstring: None,
        })
    }

    fn trait_method(owner: &str, name: &str, is_async: bool) -> Metadata {
        Metadata::TraitMethod(TraitMethodMetadata {
            module_path: "test".into(),
            trait_name: owner.into(),
            index: 0,
            name: name.into(),
            orig_name: None,
            is_async,
            inputs: vec![],
            return_type: None,
            throws: None,
            takes_self_by_arc: true,
            checksum: None,
            docstring: None,
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_sync_foreign_trait_method() {
        let items = [
            object("Signer", ObjectImpl::Trait(TraitKind::Both)),
            trait_method("Signer", "sign", false),
        ];
        let error = validate_items(&items).unwrap_err();
        assert!(error.to_string().contains("Signer.sign"));
        let items = [
            Metadata::CallbackInterface(CallbackInterfaceMetadata {
                module_path: "test".into(),
                name: "Logger".into(),
                docstring: None,
            }),
            trait_method("Logger", "write", false),
        ];
        assert!(
            validate_items(&items)
                .unwrap_err()
                .to_string()
                .contains("Logger.write")
        );
        let allowed = [
            object("LogSink", ObjectImpl::Trait(TraitKind::Both)),
            trait_method("LogSink", "log", false),
        ];
        validate_items(&allowed)?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_reader_without_async_end() {
        let items = [
            object("MessageReader", ObjectImpl::Struct),
            method("MessageReader", "end", false),
        ];
        assert!(
            validate_items(&items)
                .unwrap_err()
                .to_string()
                .contains("MessageReader")
        );
        let items = [
            object("MessageReader", ObjectImpl::Struct),
            method("MessageReader", "end", true),
        ];
        validate_items(&items)?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_object_close() {
        let items = [
            object("Client", ObjectImpl::Struct),
            method("Client", "close", true),
        ];
        assert!(
            validate_items(&items)
                .unwrap_err()
                .to_string()
                .contains("Client.close")
        );
        let items = [
            object("RustTrait", ObjectImpl::Trait(TraitKind::RustOnly)),
            trait_method("RustTrait", "close", true),
        ];
        assert!(
            validate_items(&items)
                .unwrap_err()
                .to_string()
                .contains("RustTrait.close")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_record_method() {
        let items = [
            Metadata::Record(RecordMetadata {
                module_path: "test".into(),
                name: "Options".into(),
                orig_name: None,
                remote: false,
                fields: vec![],
                docstring: None,
            }),
            method("Options", "change", false),
        ];
        assert!(
            validate_items(&items)
                .unwrap_err()
                .to_string()
                .contains("Options.change")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_object_error() {
        let items = [Metadata::Func(FnMetadata {
            module_path: "test".into(),
            name: "open".into(),
            orig_name: None,
            is_async: true,
            inputs: vec![],
            return_type: None,
            throws: Some(Type::Object {
                module_path: "test".into(),
                name: "Failure".into(),
                imp: ObjectImpl::Struct,
            }),
            checksum: None,
            docstring: None,
        })];
        assert!(
            validate_items(&items)
                .unwrap_err()
                .to_string()
                .contains("open: object Failure")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_message_objects_and_self_optional_records() -> Result<()> {
        let record = |name: &str, field_name: &str, ty| {
            Metadata::Record(RecordMetadata {
                module_path: "test".into(),
                name: name.into(),
                orig_name: None,
                remote: false,
                docstring: None,
                fields: vec![FieldMetadata {
                    name: field_name.into(),
                    orig_name: None,
                    ty,
                    default: None,
                    docstring: None,
                }],
            })
        };
        let object = Type::Object {
            module_path: "test".into(),
            name: "Handle".into(),
            imp: ObjectImpl::Struct,
        };
        assert!(
            validate_items(&[record("MessageData", "handle", object)])
                .unwrap_err()
                .to_string()
                .contains("contains an object")
        );
        let self_type = Type::Record {
            module_path: "test".into(),
            name: "ReplyParent".into(),
        };
        assert!(
            validate_items(&[record(
                "ReplyParent",
                "parent",
                Type::Optional {
                    inner_type: Box::new(self_type)
                }
            )])
            .unwrap_err()
            .to_string()
            .contains("self-typed optional")
        );
        Ok(())
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_objects_nested_in_message_records() {
        let object = Type::Object {
            module_path: "test".into(),
            name: "Handle".into(),
            imp: ObjectImpl::Struct,
        };
        let field = |name: &str, ty| FieldMetadata {
            name: name.into(),
            orig_name: None,
            ty,
            default: None,
            docstring: None,
        };
        let attachment = Metadata::Record(RecordMetadata {
            module_path: "test".into(),
            name: "Attachment".into(),
            orig_name: None,
            remote: false,
            fields: vec![field("handle", object.clone())],
            docstring: None,
        });
        let content = Metadata::Enum(EnumMetadata {
            module_path: "test".into(),
            name: "MessageContent".into(),
            orig_name: None,
            shape: EnumShape::Enum,
            remote: false,
            variants: vec![VariantMetadata {
                name: "Attachment".into(),
                orig_name: None,
                discr: None,
                fields: vec![field(
                    "attachment",
                    Type::Record {
                        module_path: "test".into(),
                        name: "Attachment".into(),
                    },
                )],
                docstring: None,
            }],
            discr_type: None,
            non_exhaustive: false,
            docstring: None,
        });
        let message = Metadata::Record(RecordMetadata {
            module_path: "test".into(),
            name: "MessageData".into(),
            orig_name: None,
            remote: false,
            fields: vec![field(
                "content",
                Type::Enum {
                    module_path: "test".into(),
                    name: "MessageContent".into(),
                },
            )],
            docstring: None,
        });
        let error = validate_items(&[attachment, content, message]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("MessageData.content: message record contains an object")
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_message_record_as_exported_argument() -> Result<()> {
        let mut action = method("Conversations", "delete_message", true);
        let Metadata::Method(ref mut value) = action else {
            unreachable!()
        };
        value.inputs = vec![FnParamMetadata::simple(
            "message",
            Type::Custom {
                module_path: "test".into(),
                name: "Message".into(),
                builtin: Box::new(Type::Record {
                    module_path: "test".into(),
                    name: "MessageData".into(),
                }),
            },
        )];
        assert!(
            validate_items(&[action])
                .unwrap_err()
                .to_string()
                .contains("pass MessageID")
        );
        let mut record_action = method("Conversations", "reply_to_message", true);
        let Metadata::Method(ref mut value) = record_action else {
            unreachable!()
        };
        value.inputs = vec![FnParamMetadata::simple(
            "message",
            Type::Record {
                module_path: "test".into(),
                name: "MessageData".into(),
            },
        )];
        assert!(
            validate_items(&[record_action])
                .unwrap_err()
                .to_string()
                .contains("pass MessageID")
        );
        let mut valid = method("Conversations", "delete_message", true);
        let Metadata::Method(ref mut value) = valid else {
            unreachable!()
        };
        value.inputs = vec![FnParamMetadata::simple("id", Type::String)];
        validate_items(&[valid])?;
        Ok(())
    }
}
