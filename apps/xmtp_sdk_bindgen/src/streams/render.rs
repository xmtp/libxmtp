use std::{collections::BTreeSet, fmt::Write as _, fs};

use anyhow::Result;
use camino::Utf8Path;
use heck::ToLowerCamelCase;

use super::{Reader, Stream};
use crate::Language;

pub(crate) fn typescript_import(streams: &[Stream], pure: bool) -> String {
    if pure || streams.is_empty() {
        return String::new();
    }
    let mut names = BTreeSet::new();
    for stream in streams {
        names.insert(stream.helper());
        names.insert(format!("type {}", stream.options));
        names.insert(format!("type {}", stream.typescript_result()));
    }
    format!(
        "import {{ {} }} from './runtime/public/streams.js';\n",
        names.into_iter().collect::<Vec<_>>().join(", ")
    )
}

pub(crate) fn typescript_exports(streams: &[Stream]) -> String {
    if streams.is_empty() {
        return String::new();
    }
    let mut names = BTreeSet::from([
        "type StreamCloseReason".to_owned(),
        "type StreamOptions".to_owned(),
    ]);
    for stream in streams {
        names.insert(stream.typescript_result().to_owned());
        names.insert(format!("type {}", stream.options));
    }
    format!(
        "export {{ {} }} from \"./runtime/public/streams.js\";\n",
        names.into_iter().collect::<Vec<_>>().join(", ")
    )
}

pub(crate) fn typescript_member(stream: &Stream) -> String {
    format!(
        "{name}(options?: {options}): {result} {{ const source = unwrap{receiver}(this); return {helper}(source, () => source.{owner}(), (selection, asyncOptions) => source.{reader}(selection, asyncOptions), options); }}\n",
        name = stream.host_name(),
        options = stream.options,
        result = stream.typescript_result(),
        receiver = stream.receiver,
        helper = stream.helper(),
        owner = stream.owner.to_lower_camel_case(),
        reader = stream.reader.to_lower_camel_case()
    )
}

const MESSAGE_DOC: &str = "/**\n * This cold Flow holds its client before and between collections. Each collection\n * opens a reader. With direct sequential collection, the next read acknowledges\n * the previous message after the collector returns. A buffer or asynchronous\n * operator can let acknowledgement start before downstream work finishes.\n * Cancellation cannot undo an acknowledgement that has started.\n *\n * Only one default message reader can own progress in a client database, across\n * all group, DM, and filter scopes. A second reader fails with\n * [XmtpException.ConsumerOwned]. An explicit `from` cursor opens independent\n * replay/live reading. It does not change default progress or create a durable\n * checkpoint for each downstream consumer.\n */\n";

pub(crate) fn documentation(stream: &Stream, language: Language) -> &'static str {
    match (language, &stream.result) {
        (Language::Swift, Reader::Conversation) => {
            "    /// The sequence holds its client only after a reader opens.\n"
        }
        (Language::Swift, Reader::Message) => {
            "    /// The next read acknowledges the previous message. The active reader holds its client.\n"
        }
        (Language::Kotlin, Reader::Conversation) => {
            "/** This cold Flow holds its client. Each collection opens a conversation reader. */\n"
        }
        (Language::Kotlin, Reader::Message) => MESSAGE_DOC,
        _ => unreachable!("native stream documentation"),
    }
}

pub(crate) fn native_member(stream: &Stream, language: Language) -> String {
    let (name, options, helper, owner, reader, receiver) = (
        stream.host_name(),
        &stream.options,
        stream.helper(),
        stream.owner.to_lower_camel_case(),
        stream.reader.to_lower_camel_case(),
        &stream.receiver,
    );
    let doc = documentation(stream, language);
    let argument = stream.argument.to_lower_camel_case();
    match language {
        Language::Swift => format!(
            "\npublic extension {receiver} {{\n{doc}    func {name}(options: {options} = .init()) async throws -> {result} {{\n        try await {helper}(ownerKey: {{ self.{owner}() }}, open: {{ try await self.{reader}({argument}: $0) }}, options: options)\n    }}\n}}\n",
            result = stream.swift_result()
        ),
        Language::Kotlin => format!(
            "\n{doc}fun {receiver}.{name}(options: {options} = {options}()): Flow<{result}> =\n    {helper}({owner}(), {{ {reader}(it) }}, options)\n",
            result = stream.kotlin_result()
        ),
        _ => unreachable!("native stream method"),
    }
}

pub(crate) fn generate_native(
    streams: &[Stream],
    language: Language,
    out: &Utf8Path,
) -> Result<()> {
    // Keep this filename: Kotlin apps can call its StreamMethodsKt holder.
    let extension = match language {
        Language::Swift => "swift",
        Language::Kotlin => "kt",
        _ => unreachable!("native streams"),
    };
    let path = out.join(format!("runtime/streams/StreamMethods.{extension}"));
    let mut code = fs::read_to_string(&path)?;
    writeln!(
        code,
        "\n// Generated from checked reader stream declarations."
    )?;
    for stream in streams {
        code.push_str(&native_member(stream, language));
    }
    code.push_str(&crate::forwarding::stream_methods(streams, language)?);
    fs::write(path, code)?;
    Ok(())
}
