//! UniFFI metadata builders for generator tests.

use uniffi_meta::{
    EnumMetadata, EnumShape, FieldMetadata, Metadata, MetadataGroup, MetadataGroupMap,
    NamespaceMetadata, RecordMetadata, Type, VariantMetadata,
};

pub(crate) const MODULE: &str = "xmtp_sdk";

/// One crate's metadata.
pub(crate) fn groups(items: Vec<Metadata>) -> MetadataGroupMap {
    MetadataGroupMap::from([(
        MODULE.into(),
        MetadataGroup {
            namespace: NamespaceMetadata {
                crate_name: MODULE.into(),
                name: MODULE.into(),
            },
            namespace_docstring: None,
            items: items.into_iter().collect(),
        },
    )])
}

pub(crate) fn field(name: &str, ty: Type, docstring: Option<&str>) -> FieldMetadata {
    FieldMetadata {
        name: name.into(),
        orig_name: None,
        ty,
        default: None,
        docstring: docstring.map(Into::into),
    }
}

pub(crate) fn record(name: &str, fields: Vec<FieldMetadata>) -> Metadata {
    Metadata::Record(RecordMetadata {
        module_path: MODULE.into(),
        name: name.into(),
        orig_name: None,
        remote: false,
        fields,
        docstring: None,
    })
}

pub(crate) fn variant(
    name: &str,
    docstring: Option<&str>,
    fields: Vec<FieldMetadata>,
) -> VariantMetadata {
    VariantMetadata {
        name: name.into(),
        orig_name: None,
        discr: None,
        fields,
        docstring: docstring.map(Into::into),
    }
}

pub(crate) fn enumeration(name: &str, variants: Vec<VariantMetadata>) -> Metadata {
    Metadata::Enum(EnumMetadata {
        module_path: MODULE.into(),
        name: name.into(),
        orig_name: None,
        shape: EnumShape::Enum,
        remote: false,
        variants,
        discr_type: None,
        non_exhaustive: false,
        docstring: None,
    })
}

pub(crate) fn record_type(name: &str) -> Type {
    Type::Record {
        module_path: MODULE.into(),
        name: name.into(),
    }
}

pub(crate) fn enum_type(name: &str) -> Type {
    Type::Enum {
        module_path: MODULE.into(),
        name: name.into(),
    }
}

pub(crate) fn optional(inner: Type) -> Type {
    Type::Optional {
        inner_type: Box::new(inner),
    }
}

pub(crate) fn sequence(inner: Type) -> Type {
    Type::Sequence {
        inner_type: Box::new(inner),
    }
}
