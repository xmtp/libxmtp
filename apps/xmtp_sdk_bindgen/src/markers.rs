//! Metadata markers in façade docstrings.
//!
//! `xmtp_macro::sdk_export` turns its `pure` argument and `#[sdk(...)]`
//! options into `#[doc = "@xmtp-..."]` lines, and UniFFI carries them into
//! the library metadata. The generator reads facts from these markers instead
//! of keeping tables of façade names, and strips them from generated
//! documentation. `apps/xmtp_sdk_bindgen/README.md` lists the vocabulary.

use std::{borrow::Cow, ops::Range};

use anyhow::{Result, bail};
use uniffi_meta::{FieldMetadata, Metadata, MetadataGroupMap};

/// A synchronous getter whose value never changes for the object's lifetime.
pub(crate) const IMMUTABLE: &str = "@xmtp-immutable";
/// The public string of an enum variant: `@xmtp-kind=conversation.joined`.
pub(crate) const KIND: &str = "@xmtp-kind";
/// A synchronous function of the browser's main-thread module.
pub(crate) const PURE: &str = "@xmtp-pure";
/// A call the browser worker makes itself; it never crosses the bridge.
pub(crate) const WORKER: &str = "@xmtp-worker";
/// A private item that the public projection leaves out.
pub(crate) const INTERNAL: &str = "@xmtp-internal";
/// A field that diagnostic text hides: `@xmtp-redact`, or
/// `@xmtp-redact=secret` for one key of a string map.
pub(crate) const REDACT: &str = "@xmtp-redact";
/// A record or enum whose redacted fields the macro checked. The macro
/// rejects both redaction markers in a doc comment, so `@xmtp-redact` counts
/// only beside this one.
pub(crate) const REDACTED: &str = "@xmtp-redacted";

/// Every marker the generator reads.
const VOCABULARY: &[&str] = &[IMMUTABLE, INTERNAL, KIND, PURE, REDACT, REDACTED, WORKER];

const PREFIX: &str = "@xmtp-";

/// A marker is one whitespace-separated word: `@xmtp-`, a lowercase name,
/// and an optional `=value`. Returns the name with its prefix and the value.
/// Prose such as `@xmtp-org/pkg` is not a marker.
fn marker(word: &str) -> Option<(&str, Option<&str>)> {
    let rest = word.strip_prefix(PREFIX)?;
    let (name, value) = match rest.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (rest, None),
    };
    let valid = name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_lowercase() || c == '-')
        && value.is_none_or(|value| !value.is_empty());
    valid.then(|| (&word[..PREFIX.len() + name.len()], value))
}

/// The markers of a docstring. `@xmtp-kind=a.b` yields
/// `("@xmtp-kind", Some("a.b"))`.
fn markers(doc: &str) -> impl Iterator<Item = (&str, Option<&str>)> {
    doc.split_whitespace().filter_map(marker)
}

/// Whether a docstring carries the marker.
pub(crate) fn has(doc: Option<&str>, name: &str) -> bool {
    doc.is_some_and(|doc| markers(doc).any(|(found, _)| found == name))
}

/// The value of a `marker=value` entry.
pub(crate) fn value<'a>(doc: Option<&'a str>, name: &str) -> Option<&'a str> {
    markers(doc?).find_map(|(found, value)| (found == name).then_some(value)?)
}

/// What a redacted field hides.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Redaction {
    /// The whole value.
    Whole,
    /// One key of a string map.
    Key(String),
}

/// The field's `#[sdk(redact)]`, if any.
pub(crate) fn redaction(field: &FieldMetadata) -> Option<Redaction> {
    markers(field.docstring.as_deref()?)
        .find(|(name, _)| *name == REDACT)
        .map(|(_, key)| match key {
            Some(key) => Redaction::Key(key.to_owned()),
            None => Redaction::Whole,
        })
}

/// Every docstring of an item, with the name an error should report.
fn docstrings(item: &Metadata) -> Vec<(String, Option<&str>)> {
    fn named<'a>(owner: &str, name: &str, doc: &'a Option<String>) -> (String, Option<&'a str>) {
        (format!("{owner}.{name}"), doc.as_deref())
    }
    match item {
        Metadata::Func(function) => vec![(function.name.clone(), function.docstring.as_deref())],
        Metadata::Constructor(call) => vec![named(&call.self_name, &call.name, &call.docstring)],
        Metadata::Method(call) => vec![named(&call.self_name, &call.name, &call.docstring)],
        Metadata::TraitMethod(call) => vec![named(&call.trait_name, &call.name, &call.docstring)],
        Metadata::Object(object) => vec![(object.name.clone(), object.docstring.as_deref())],
        Metadata::CallbackInterface(callback) => {
            vec![(callback.name.clone(), callback.docstring.as_deref())]
        }
        Metadata::CustomType(custom) => vec![(custom.name.clone(), custom.docstring.as_deref())],
        Metadata::Record(record) => {
            std::iter::once((record.name.clone(), record.docstring.as_deref()))
                .chain(
                    record
                        .fields
                        .iter()
                        .map(|field| named(&record.name, &field.name, &field.docstring)),
                )
                .collect()
        }
        Metadata::Enum(value) => {
            let mut docs = vec![(value.name.clone(), value.docstring.as_deref())];
            for variant in &value.variants {
                let owner = format!("{}.{}", value.name, variant.name);
                docs.push((owner.clone(), variant.docstring.as_deref()));
                for field in &variant.fields {
                    docs.push(named(&owner, &field.name, &field.docstring));
                }
            }
            docs
        }
        _ => Vec::new(),
    }
}

/// Stop on marker text that does not work as a marker. A misspelled
/// `@xmtp-internal`, or one with punctuation attached (`@xmtp-internal.`),
/// would otherwise leave a private item public without a word. Only a
/// package path such as `@xmtp-org/pkg` may use the prefix in prose.
fn check_doc(owner: &str, doc: &str) -> Result<()> {
    for word in doc.split_whitespace() {
        for (at, _) in word.match_indices(PREFIX) {
            let rest = &word[at + PREFIX.len()..];
            if !rest.starts_with(|c: char| c.is_ascii_lowercase()) {
                continue;
            }
            let name = rest
                .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
                .unwrap_or(rest.len());
            if rest[name..].starts_with('/') {
                continue;
            }
            match marker(word) {
                Some((name, value)) if at == 0 && VOCABULARY.contains(&name) => {
                    check_value(owner, name, value)?;
                }
                Some((name, _)) if at == 0 => bail!(
                    "{owner}: unknown metadata marker {name}; the generator reads {}",
                    VOCABULARY.join(", ")
                ),
                _ => bail!(
                    "{owner}: `{word}` is not a metadata marker; write the marker as a word of \
                     its own, one of {}",
                    VOCABULARY.join(", ")
                ),
            }
        }
    }
    Ok(())
}

/// A marker value reaches generated string literals, and a doc comment can
/// spell out a marker without the macro's checks, so the generator checks
/// the value again. A kind is what `#[sdk(kind = "...")]` admits: lowercase
/// letters, digits, `_`, and `.`. A redacted map key is what
/// `#[sdk(redact = "...")]` admits: ASCII letters, digits, `_`, `.`, and `-`.
/// The other markers take no value.
fn check_value(owner: &str, name: &str, value: Option<&str>) -> Result<()> {
    match (name, value) {
        (KIND, Some(kind))
            if kind
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.') =>
        {
            Ok(())
        }
        (KIND, _) => bail!(
            "{owner}: {KIND} needs a kind of lowercase letters, digits, `_`, and `.`, such as \
             {KIND}=conversation.joined"
        ),
        (REDACT, Some(key))
            if key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')) =>
        {
            Ok(())
        }
        (REDACT, Some(_)) => {
            bail!("{owner}: a {REDACT} map key holds ASCII letters, digits, `_`, `.`, and `-`")
        }
        (_, Some(_)) => bail!("{owner}: {name} takes no value"),
        (_, None) => Ok(()),
    }
}

/// A redacted field relies on the macro's checks: its siblings say whether
/// they print, and the type's `Debug` hides it too. Only a type that went
/// through `sdk_export` carries `@xmtp-redacted`.
fn check_redaction(item: &Metadata) -> Result<()> {
    // Each field with the name an error reports.
    let (doc, fields): (_, Vec<(String, &FieldMetadata)>) = match item {
        Metadata::Record(record) => (
            record.docstring.as_deref(),
            record
                .fields
                .iter()
                .map(|field| (record.name.clone(), field))
                .collect(),
        ),
        Metadata::Enum(value) => (
            value.docstring.as_deref(),
            value
                .variants
                .iter()
                .flat_map(|variant| {
                    let owner = format!("{}.{}", value.name, variant.name);
                    variant
                        .fields
                        .iter()
                        .map(move |field| (owner.clone(), field))
                })
                .collect(),
        ),
        _ => return Ok(()),
    };
    match fields.iter().find(|(_, field)| redaction(field).is_some()) {
        Some((owner, field)) if !has(doc, REDACTED) => bail!(
            "{owner}.{}: {REDACT} without {REDACTED}; mark the field #[sdk(redact)] under \
             #[xmtp_macro::sdk_export]",
            field.name
        ),
        _ => Ok(()),
    }
}

/// Check the markers of every docstring in the metadata.
pub(crate) fn validate(groups: &MetadataGroupMap) -> Result<()> {
    for group in groups.values() {
        let namespace = (
            group.namespace.name.clone(),
            group.namespace_docstring.as_deref(),
        );
        for (owner, doc) in
            std::iter::once(namespace).chain(group.items.iter().flat_map(docstrings))
        {
            check_doc(&owner, doc.unwrap_or_default())?;
        }
        for item in &group.items {
            check_redaction(item)?;
        }
    }
    Ok(())
}

/// The documentation text of a generated line: the part inside a `/** */`
/// comment or after `///`. `in_block` carries an open `/**` to the next
/// line. Code, and the code after a closing `*/`, is not documentation.
fn doc_text(line: &str, in_block: &mut bool) -> Option<Range<usize>> {
    let body = line.trim_end_matches(['\n', '\r']);
    let indent = body.len() - body.trim_start().len();
    let start = if *in_block {
        indent
    } else if body[indent..].starts_with("///") {
        return Some(indent + 3..body.len());
    } else if body[indent..].starts_with("/**") {
        *in_block = true;
        indent + 3
    } else {
        return None;
    };
    match body[start..].find("*/") {
        Some(end) => {
            *in_block = false;
            Some(start..start + end)
        }
        None => Some(start..body.len()),
    }
}

/// A line that holds only comment syntax, as UniFFI writes an empty
/// documentation line.
fn empty_comment(line: &str) -> bool {
    line.trim()
        .trim_start_matches("///")
        .trim_start_matches("//")
        .trim_start_matches('*')
        .trim()
        .is_empty()
}

/// Documentation text without its marker words, each with the space before it.
fn without_markers(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let space = rest.len() - rest.trim_start().len();
        let word = rest[space..]
            .find(char::is_whitespace)
            .map_or(rest.len(), |end| space + end);
        if marker(&rest[space..word]).is_none() {
            output.push_str(&rest[..word]);
        }
        rest = &rest[word..];
    }
    output
}

/// One generated line without the markers in `doc`. A documentation line
/// that held only markers becomes empty.
fn strip_line(line: &str, doc: Range<usize>) -> String {
    let ending = &line[line.trim_end_matches(['\n', '\r']).len()..];
    let text = format!(
        "{}{}{}",
        &line[..doc.start],
        without_markers(&line[doc.clone()]),
        &line[doc.end..line.len() - ending.len()]
    );
    if empty_comment(&text) {
        String::new()
    } else {
        format!("{}{ending}", text.trim_end())
    }
}

/// Generated source without markers in its documentation. Code lines and
/// string literals keep their text. A `/** */` comment that held only
/// markers goes too, so an item documented by markers alone reads as if it
/// had no documentation.
pub(crate) fn strip(source: &str) -> String {
    let mut in_block = false;
    // Each line, and whether it held a marker.
    let lines = source
        .split_inclusive('\n')
        .map(|line| match doc_text(line, &mut in_block) {
            Some(doc) if markers(&line[doc.clone()]).next().is_some() => {
                (Cow::Owned(strip_line(line, doc)), true)
            }
            _ => (Cow::Borrowed(line), false),
        })
        .collect::<Vec<_>>();
    let mut output = String::with_capacity(source.len());
    let mut index = 0;
    while index < lines.len() {
        let line = &lines[index].0;
        if line.trim() == "/**"
            && let Some(len) = lines[index + 1..]
                .iter()
                .position(|(line, _)| line.trim_start().starts_with("*/"))
        {
            let body = &lines[index + 1..index + 1 + len];
            if body.iter().any(|(_, marked)| *marked)
                && body.iter().all(|(line, _)| empty_comment(line))
            {
                // UniFFI writes a newline and the comment in front of the
                // declaration. Kotlin puts some declarations on the closing
                // line, so they rejoin the line before the comment.
                let code = lines[index + 1 + len].0.trim_start()["*/".len()..].to_owned();
                if !code.trim().is_empty() && output.ends_with('\n') {
                    output.pop();
                    output.push_str(&code);
                }
                index += len + 2;
                continue;
            }
        }
        output.push_str(line);
        index += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use uniffi_meta::Type;

    use super::*;
    use crate::test_metadata::{enumeration, field, groups, record, variant};

    #[xmtp_common::test(unwrap_try = true)]
    fn markers_are_read_by_exact_name_with_optional_value() {
        let doc = Some("The key.\n@xmtp-immutable\n@xmtp-kind=hmac_keys.updated @xmtp-internal");
        assert!(has(doc, IMMUTABLE));
        assert!(has(doc, INTERNAL));
        assert!(!has(doc, "@xmtp-intern"));
        assert!(!has(doc, PURE));
        assert_eq!(value(doc, KIND), Some("hmac_keys.updated"));
        assert_eq!(value(doc, IMMUTABLE), None);
        assert_eq!(value(Some("@xmtp-kind"), KIND), None);
        assert!(!has(None, IMMUTABLE));
        // A marker is a whole word; prose that merely starts like one is not.
        assert!(!has(Some("See @xmtp-pure/docs or \"@xmtp-pure\"."), PURE));
    }

    // A misspelled marker must not silently change what an item is.
    #[xmtp_common::test(unwrap_try = true)]
    fn unknown_markers_stop_generation_and_name_the_vocabulary() {
        let known = groups(vec![
            record(
                "Options",
                vec![field("key", Type::String, Some("The key. @xmtp-internal"))],
            ),
            enumeration(
                "EventKind",
                vec![variant("Lagged", Some("@xmtp-kind=lagged"), vec![])],
            ),
        ]);
        validate(&known)?;
        // Prose that only looks like a marker is fine.
        validate(&groups(vec![record(
            "Package",
            vec![field("name", Type::String, Some("Such as @xmtp-org/pkg."))],
        )]))?;

        let misspelled = groups(vec![record(
            "Options",
            vec![field("key", Type::String, Some("The key. @xmtp-interal"))],
        )]);
        let error = validate(&misspelled).unwrap_err().to_string();
        assert!(
            error.starts_with("Options.key: unknown metadata marker @xmtp-interal;"),
            "{error}"
        );
        assert!(error.contains(
            "@xmtp-immutable, @xmtp-internal, @xmtp-kind, @xmtp-pure, @xmtp-redact, \
             @xmtp-redacted, @xmtp-worker"
        ));

        // A marker with punctuation attached is not a marker; it must not
        // silently stop working.
        for doc in [
            "The key. @xmtp-internal.",
            "The key (@xmtp-internal).",
            "@xmtp-worker, @xmtp-internal",
            "See x@xmtp-pure",
            "@xmtp-kind=",
        ] {
            let error = check_doc("Options.key", doc).unwrap_err().to_string();
            assert!(
                error.contains("is not a metadata marker; write the marker as a word of its own"),
                "{doc}: {error}"
            );
        }
        // A marker value reaches generated string literals, so a kind keeps
        // the macro's grammar and the other markers take no value.
        for doc in [
            "@xmtp-kind=x';globalThis.alert(1);//",
            "@xmtp-kind=Conversation.Joined",
            "@xmtp-kind",
        ] {
            let error = check_doc("EventKind.Lagged", doc).unwrap_err().to_string();
            assert!(
                error.starts_with("EventKind.Lagged: @xmtp-kind needs a kind of lowercase"),
                "{doc}: {error}"
            );
        }
        let error = check_doc("Client.id", "@xmtp-immutable=yes").unwrap_err();
        assert_eq!(
            error.to_string(),
            "Client.id: @xmtp-immutable takes no value"
        );
        check_doc("EventKind.HmacKeysUpdated", "@xmtp-kind=hmac_keys.updated2")?;
        check_doc("EncodedContent.parameters", "@xmtp-redact=x-Secret.v1_2")?;
        for key in ["a\"b", "a'b", "${x}", "a\\b"] {
            let error = check_doc("EncodedContent.parameters", &format!("@xmtp-redact={key}"))
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("map key holds ASCII letters"),
                "{key}: {error}"
            );
        }
        let error = check_doc("Session", "@xmtp-redacted=1").unwrap_err();
        assert_eq!(error.to_string(), "Session: @xmtp-redacted takes no value");
        // A package path is prose, whatever follows it.
        for doc in ["(@xmtp-org/pkg).", "@xmtp-org2/pkg-name,", "see:@xmtp-a/b"] {
            check_doc("Options.key", doc)?;
        }

        let variant_field = groups(vec![enumeration(
            "Channel",
            vec![variant(
                "Apns",
                None,
                vec![field("token", Type::String, Some("@xmtp-secret"))],
            )],
        )]);
        let error = validate(&variant_field).unwrap_err().to_string();
        assert!(error.starts_with("Channel.Apns.token: unknown metadata marker @xmtp-secret"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn redaction_distinguishes_a_whole_field_from_a_map_key() {
        let redact = |doc: Option<&str>| redaction(&field("value", Type::String, doc));
        assert_eq!(redact(Some("@xmtp-redact")), Some(Redaction::Whole));
        assert_eq!(
            redact(Some("Parameters.\n@xmtp-redact=secret")),
            Some(Redaction::Key("secret".into()))
        );
        assert_eq!(redact(Some("@xmtp-redacted")), None);
        assert_eq!(redact(Some("Plain.")), None);
        assert_eq!(redact(None), None);
    }

    // The macro checks a redacted field's siblings and implements the type's
    // Debug, and it stamps the type. A redaction marker written by hand on an
    // unstamped type skipped those checks, so generation stops.
    #[xmtp_common::test(unwrap_try = true)]
    fn redaction_counts_only_on_a_type_the_macro_stamped() {
        let secret = || field("token", Type::String, Some("@xmtp-redact"));
        let items = || {
            vec![
                record(
                    "Session",
                    vec![field("label", Type::String, None), secret()],
                ),
                enumeration("Channel", vec![variant("Apns", None, vec![secret()])]),
            ]
        };
        let mut stamped = items();
        for item in &mut stamped {
            match item {
                Metadata::Record(record) => {
                    record.docstring = Some("A session.\n@xmtp-redacted".into())
                }
                Metadata::Enum(value) => value.docstring = Some("@xmtp-redacted".into()),
                _ => unreachable!(),
            }
        }
        validate(&groups(stamped))?;
        for (item, owner) in items().into_iter().zip(["Session", "Channel.Apns"]) {
            let error = validate(&groups(vec![item])).unwrap_err().to_string();
            assert_eq!(
                error,
                format!(
                    "{owner}.token: @xmtp-redact without @xmtp-redacted; mark the field \
                     #[sdk(redact)] under #[xmtp_macro::sdk_export]"
                )
            );
        }
    }

    // UniFFI writes each docstring line as ` * line` in a `/** */` block. A
    // block of markers alone disappears; a marker beside text leaves the text.
    #[xmtp_common::test(unwrap_try = true)]
    fn strip_removes_markers_and_the_comments_they_leave_empty() {
        let source = "\
/**
 * @xmtp-pure
 */
public func encodeText() {}
    /**
     * The ID.
     * @xmtp-immutable
     */
    func id() -> String
/**
 * Private host admission receipt. @xmtp-worker @xmtp-internal
 */
/**
 * @xmtp-worker Reports a store.
 */
/**
 * @xmtp-kind=conversation.joined
 */
case conversationJoined
/**
 * Kept.
 *
 * Paragraph.
 */
/**
 */
";
        assert_eq!(
            strip(source),
            "\
public func encodeText() {}
    /**
     * The ID.
     */
    func id() -> String
/**
 * Private host admission receipt.
 */
/**
 * Reports a store.
 */
case conversationJoined
/**
 * Kept.
 *
 * Paragraph.
 */
/**
 */
"
        );
        // Kotlin writes some declarations on the closing line. They rejoin the
        // line before the comment, as if UniFFI had written no comment.
        assert_eq!(
            strip(
                "    }\n\n    \n    /**\n     * @xmtp-immutable\n     */override fun `archives`(): Archives {\n    \n    /**\n     * The ID.\n     * @xmtp-immutable\n     */override fun `id`(): ConversationId {\n\n        /**\n         * @xmtp-pure\n         */ fun `sdkVersion`(): kotlin.String {\n"
            ),
            "    }\n\n    override fun `archives`(): Archives {\n    \n    /**\n     * The ID.\n     */override fun `id`(): ConversationId {\n fun `sdkVersion`(): kotlin.String {\n"
        );
        // A declaration on the closing line keeps the space before the comment.
        assert_eq!(
            strip(
                "    public init(attachmentKey: String, \n        /**\n         * @xmtp-kind=a\n         */url: String) {\n"
            ),
            "    public init(attachmentKey: String, url: String) {\n"
        );
        assert_eq!(strip("no markers"), "no markers");
        assert_eq!(strip("/// @xmtp-immutable\nfn id()"), "fn id()");
        assert_eq!(strip("/** One line. @xmtp-pure */\n"), "/** One line. */\n");
    }

    // Only documentation loses markers. Code, string literals in code, and
    // prose that only looks like a marker stay as they are.
    #[xmtp_common::test(unwrap_try = true)]
    fn strip_leaves_code_strings_and_prose() {
        let source = "\
let tag = \"see @xmtp-pure here\";
const MARKER = '@xmtp-pure';
// @xmtp-worker in a plain comment
/**
 * Install @xmtp-org/pkg first. @xmtp-pure
 */
fun run() = \"@xmtp-immutable\" /* @xmtp-immutable */
/** @xmtp-pure */ val x = \" @xmtp-pure \"
";
        assert_eq!(
            strip(source),
            "\
let tag = \"see @xmtp-pure here\";
const MARKER = '@xmtp-pure';
// @xmtp-worker in a plain comment
/**
 * Install @xmtp-org/pkg first.
 */
fun run() = \"@xmtp-immutable\" /* @xmtp-immutable */
/** */ val x = \" @xmtp-pure \"
"
        );
    }
}
