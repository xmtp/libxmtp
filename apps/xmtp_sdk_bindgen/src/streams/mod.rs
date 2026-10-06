//! Checked host stream methods attached to exported reader methods.

mod render;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use heck::ToLowerCamelCase;
use uniffi_meta::{Metadata, MethodMetadata, Type};

use crate::markers;
pub(crate) use render::{
    documentation, generate_native, typescript_exports, typescript_import, typescript_member,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reader {
    Conversation,
    Message,
}

/// The adapter policy is typed runtime code. Metadata selects its existing
/// options type, receiver, private owner getter, and reader opener.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Stream {
    pub receiver: String,
    pub name: String,
    pub reader: String,
    pub argument: String,
    pub owner: String,
    pub options: String,
    pub input: String,
    pub result: Reader,
}

impl Stream {
    pub fn host_name(&self) -> String {
        self.name.to_lower_camel_case()
    }
    pub fn helper(&self) -> String {
        format!("open{}", self.options)
    }
    pub fn typescript_result(&self) -> &'static str {
        match self.result {
            Reader::Conversation => "ConversationStream",
            Reader::Message => "MessageStream",
        }
    }
    pub fn swift_result(&self) -> &'static str {
        match self.result {
            Reader::Conversation => "SDKConversationStream",
            Reader::Message => "SDKMessageStream",
        }
    }
    pub fn kotlin_result(&self) -> &'static str {
        match self.result {
            Reader::Conversation => "Conversation",
            Reader::Message => "Message",
        }
    }
}

/// Keep the marker grammar independent of any host expression syntax.
pub(crate) fn parameters(value: &str) -> Result<[&str; 3]> {
    let values = value.split(':').collect::<Vec<_>>();
    if values.len() != 3
        || values.iter().any(|value| {
            let mut chars = value.chars();
            !chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
    {
        bail!("stream marker needs name:options:owner ASCII identifiers");
    }
    Ok([values[0], values[1], values[2]])
}

pub(crate) fn resolve(items: &[&Metadata]) -> Result<Vec<Stream>> {
    let mut streams = Vec::new();
    let mut names = BTreeSet::new();
    for item in items {
        let Metadata::Method(method) = item else {
            continue;
        };
        let Some(value) = markers::value(method.docstring.as_deref(), markers::STREAM) else {
            continue;
        };
        let [name, options, owner] = parameters(value)?;
        let label = format!("{}.{}", method.self_name, method.name);
        if markers::has(method.docstring.as_deref(), markers::INTERNAL) {
            bail!("{label}: a stream reader must remain public");
        }
        if items.iter().filter(|item| matches!(item, Metadata::Object(object) if object.name == method.self_name && object.imp.has_struct())).count() != 1 {
            bail!("{label}: stream receiver needs one exported object");
        }
        let (input, result) =
            reader_shape(method).with_context(|| format!("{label}: invalid stream reader"))?;
        let reader_type = match result {
            Reader::Conversation => "ConversationReader",
            Reader::Message => "MessageReader",
        };
        if items.iter().filter(|item| matches!(item, Metadata::Object(object) if object.name == reader_type && object.imp.has_struct())).count() != 1 {
            bail!("{label}: stream result needs one exported {reader_type} object");
        }
        let expected = match options {
            "ConversationStreamOptions" => ("ConversationReaderOptions", Reader::Conversation),
            "MessageStreamOptions" => ("MessageReaderOptions", Reader::Message),
            "ConversationMessageStreamOptions" => {
                ("ConversationMessageReaderOptions", Reader::Message)
            }
            _ => bail!("{label}: unsupported stream options {options}"),
        };
        if (input.as_str(), &result) != (expected.0, &expected.1) {
            bail!("{label}: {options} does not match its reader input and result");
        }
        if items
            .iter()
            .filter(|item| matches!(item, Metadata::Record(record) if record.name == input))
            .count()
            != 1
        {
            bail!("{label}: stream input record {input} is missing or duplicated");
        }
        let owners = items
            .iter()
            .filter_map(|item| match item {
                Metadata::Method(candidate)
                    if candidate.self_name == method.self_name && candidate.name == owner =>
                {
                    Some(candidate)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if owners.len() != 1 || !valid_owner(owners[0]) {
            bail!(
                "{label}: owner {owner} needs one immutable, host-internal, argument-free u64 method"
            );
        }
        let host_name = name.to_lower_camel_case();
        if host_name == "constructor"
            || !names.insert((method.self_name.clone(), host_name.clone()))
        {
            bail!("{label}: duplicate or reserved public stream name {host_name}");
        }
        if items.iter().any(|item| match item {
            Metadata::Method(candidate) => {
                candidate.self_name == method.self_name
                    && candidate.name.to_lower_camel_case() == host_name
            }
            Metadata::Constructor(candidate) => {
                candidate.self_name == method.self_name
                    && candidate.name.to_lower_camel_case() == host_name
            }
            _ => false,
        }) {
            bail!("{label}: public stream name {host_name} collides with an exported method");
        }
        let stream = Stream {
            receiver: method.self_name.clone(),
            name: name.into(),
            reader: method.name.clone(),
            argument: method.inputs[0].name.clone(),
            owner: owner.into(),
            options: options.into(),
            input,
            result,
        };
        if items.iter().any(|item| {
            exported_type_name(item)
                .is_some_and(|name| name == options || name == stream.typescript_result())
        }) {
            bail!("{label}: host stream type collides with an exported type");
        }
        streams.push(stream);
    }
    streams.sort_by(|a, b| (&a.receiver, &a.name).cmp(&(&b.receiver, &b.name)));
    common(&streams, "Group", "Dm")?;
    Ok(streams)
}

fn exported_type_name(item: &Metadata) -> Option<&str> {
    match item {
        Metadata::Record(value) => Some(&value.name),
        Metadata::Enum(value) => Some(&value.name),
        Metadata::Object(value) => Some(&value.name),
        Metadata::CustomType(value) => Some(&value.name),
        Metadata::CallbackInterface(value) => Some(&value.name),
        _ => None,
    }
}

fn valid_owner(method: &MethodMetadata) -> bool {
    !method.is_async
        && method.inputs.is_empty()
        && method.throws.is_none()
        && matches!(method.return_type, Some(Type::UInt64))
        && [
            markers::HOST_INTERNAL,
            markers::INTERNAL,
            markers::IMMUTABLE,
        ]
        .iter()
        .all(|marker| markers::has(method.docstring.as_deref(), marker))
}

fn reader_shape(method: &MethodMetadata) -> Result<(String, Reader)> {
    if !method.is_async
        || method.inputs.len() != 1
        || !matches!(&method.throws, Some(Type::Enum { name, .. }) if name == "XmtpError")
    {
        bail!("expected async fallible method with one input and XmtpError");
    }
    let Type::Optional { inner_type } = &method.inputs[0].ty else {
        bail!("expected optional record input")
    };
    let Type::Record { name, .. } = inner_type.as_ref() else {
        bail!("expected optional record input")
    };
    let result = match &method.return_type {
        Some(Type::Object { name, imp, .. }) if name == "MessageReader" && imp.has_struct() => {
            Reader::Message
        }
        Some(Type::Object { name, imp, .. })
            if name == "ConversationReader" && imp.has_struct() =>
        {
            Reader::Conversation
        }
        _ => bail!("expected MessageReader or ConversationReader result"),
    };
    Ok((name.clone(), result))
}

/// Forward only common host methods. Each receiver keeps its own checked
/// opener and owner method; the common value forwards to that receiver.
pub(crate) fn common<'a>(
    streams: &'a [Stream],
    first: &str,
    second: &str,
) -> Result<Vec<&'a Stream>> {
    let mut common = Vec::new();
    for left in streams.iter().filter(|stream| stream.receiver == first) {
        if let Some(right) = streams
            .iter()
            .find(|stream| stream.receiver == second && stream.host_name() == left.host_name())
        {
            if left.options != right.options
                || left.input != right.input
                || left.result != right.result
            {
                bail!(
                    "{first}/{second}.{}: conflicting common stream signatures",
                    left.host_name()
                );
            }
            common.push(left);
        }
    }
    Ok(common)
}
