//! The public projection of client events.
//!
//! The events spec gives each event kind a public string, spells the values
//! of event enums in snake_case, and keeps the Rust spelling of payload
//! fields. All three come from the façade metadata, so a new event variant,
//! payload record, or cause enum needs no generator edit.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use heck::ToSnakeCase;
use uniffi_meta::{Metadata, Type, VariantMetadata};

use super::camel;
use crate::markers;

/// The enum whose variants are the emitted events. Its payload fields keep
/// their Rust spelling.
pub(super) const EVENT_ENUM: &str = "ClientEvent";
/// The enum whose `#[sdk(kind = "...")]` markers name every event kind.
const EVENT_KIND_ENUM: &str = "EventKind";
/// The record that selects events.
const EVENT_FILTER: &str = "EventFilter";

/// Add every record and enum that `ty` reaches through record fields and
/// enum variants to `seen`. The walk does not enter a type named in `stop`.
fn reach(ty: &Type, items: &[&Metadata], stop: &[&str], seen: &mut BTreeSet<String>) {
    match ty {
        Type::Record { name, .. } | Type::Enum { name, .. } => {
            if stop.contains(&name.as_str()) || !seen.insert(name.clone()) {
                return;
            }
            for item in items {
                let fields = match item {
                    Metadata::Record(record) if &record.name == name => {
                        record.fields.iter().collect()
                    }
                    Metadata::Enum(value) if &value.name == name => value
                        .variants
                        .iter()
                        .flat_map(|variant| &variant.fields)
                        .collect(),
                    _ => Vec::new(),
                };
                for field in fields {
                    reach(&field.ty, items, stop, seen);
                }
            }
        }
        Type::Optional { inner_type }
        | Type::Sequence { inner_type }
        | Type::Set { inner_type }
        | Type::Box { inner_type } => reach(inner_type, items, stop, seen),
        Type::Map {
            key_type,
            value_type,
        } => {
            reach(key_type, items, stop, seen);
            reach(value_type, items, stop, seen);
        }
        _ => {}
    }
}

/// The argument, result, and error types of a call.
fn call_types(item: &Metadata) -> Vec<&Type> {
    let (inputs, output, throws) = match item {
        Metadata::Func(call) => (&call.inputs, call.return_type.as_ref(), &call.throws),
        Metadata::Method(call) => (&call.inputs, call.return_type.as_ref(), &call.throws),
        Metadata::TraitMethod(call) => (&call.inputs, call.return_type.as_ref(), &call.throws),
        Metadata::Constructor(call) => (&call.inputs, None, &call.throws),
        _ => return Vec::new(),
    };
    inputs
        .iter()
        .map(|input| &input.ty)
        .chain(output)
        .chain(throws)
        .collect()
}

/// What the events spec fixes about the public types.
pub(super) struct Events {
    /// Records that `ClientEvent` payloads and `EventFilter` carry, nested
    /// ones included. Their TypeScript fields keep the Rust spelling.
    records: BTreeSet<String>,
    /// Enums that the same payloads carry. Their values are snake_case.
    enums: BTreeSet<String>,
    /// The public string of each `EventKind` variant, by variant name.
    kinds: BTreeMap<String, String>,
}

impl Events {
    pub(super) fn new(items: &[&Metadata]) -> Result<Self> {
        let (records, enums) = event_types(items)?;
        Ok(Self {
            records,
            enums,
            kinds: event_kinds(items)?,
        })
    }

    /// Whether a record keeps the Rust spelling of its fields.
    pub(super) fn keeps_rust_fields(&self, record: &str) -> bool {
        self.records.contains(record)
    }

    /// The public string of an enum variant. `ClientEvent` uses the kind of
    /// the `EventKind` variant with its name. Any other enum uses its
    /// variant's `#[sdk(kind = "...")]`, else snake_case when an event
    /// carries it, else camelCase.
    pub(super) fn variant_kind(&self, owner: &str, variant: &VariantMetadata) -> Result<String> {
        let marked = markers::value(variant.docstring.as_deref(), markers::KIND);
        if owner == EVENT_ENUM {
            if marked.is_some() {
                bail!(
                    "{owner}.{}: {EVENT_ENUM} takes its kinds from {EVENT_KIND_ENUM}; remove its #[sdk(kind = \"...\")]",
                    variant.name
                );
            }
            return match self.kinds.get(&variant.name) {
                Some(kind) => Ok(kind.clone()),
                None => bail!(
                    "{owner}.{}: {EVENT_KIND_ENUM} has no variant with this name",
                    variant.name
                ),
            };
        }
        Ok(match marked {
            Some(kind) => kind.to_owned(),
            None if self.enums.contains(owner) => variant.name.to_snake_case(),
            None => camel(&variant.name),
        })
    }
}

/// The records and enums that `ClientEvent` payloads and `EventFilter`
/// reach. A record that another call also reaches would need both field
/// spellings, so generation stops: give the event its own record. So does a
/// shared enum whose snake_case and camelCase values differ. A shared enum
/// with one-word values, such as `ConnectionState`, spells them the same.
fn event_types(items: &[&Metadata]) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let mut events = BTreeSet::new();
    let mut other = BTreeSet::new();
    for item in items {
        match item {
            Metadata::Enum(value) if value.name == EVENT_ENUM => {
                for field in value.variants.iter().flat_map(|variant| &variant.fields) {
                    reach(&field.ty, items, &[], &mut events);
                }
            }
            Metadata::Record(record) if record.name == EVENT_FILTER => {
                events.insert(record.name.clone());
                for field in &record.fields {
                    reach(&field.ty, items, &[], &mut events);
                }
            }
            _ => {
                for ty in call_types(item) {
                    reach(ty, items, &[EVENT_ENUM, EVENT_FILTER], &mut other);
                }
            }
        }
    }
    let (records, enums) = events.into_iter().partition::<BTreeSet<_>, _>(|name| {
        items
            .iter()
            .any(|item| matches!(item, Metadata::Record(record) if &record.name == name))
    });
    if let Some(shared) = records.intersection(&other).next() {
        bail!(
            "{shared}: an event payload and another call share this record, and they spell \
             its TypeScript fields differently; give the event its own record"
        );
    }
    for shared in enums.intersection(&other) {
        let variants = items.iter().flat_map(|item| match item {
            Metadata::Enum(value) if &value.name == shared => value.variants.as_slice(),
            _ => &[],
        });
        for variant in variants {
            let marked = markers::value(variant.docstring.as_deref(), markers::KIND).is_some();
            if !marked && variant.name.to_snake_case() != camel(&variant.name) {
                bail!(
                    "{shared}.{}: an event payload and another call share this enum, and they \
                     spell this TypeScript value differently; give the event its own enum",
                    variant.name
                );
            }
        }
    }
    Ok((records, enums))
}

/// The filter kind and the emitted event kind use the same public string.
/// Each `EventKind` variant names it once, and each `ClientEvent` variant
/// takes the kind of the `EventKind` variant with its name.
fn event_kinds(items: &[&Metadata]) -> Result<BTreeMap<String, String>> {
    let variants = |name: &str| {
        items.iter().find_map(|item| match item {
            Metadata::Enum(value) if value.name == name => Some(&value.variants),
            _ => None,
        })
    };
    let mut kinds = BTreeMap::new();
    for variant in variants(EVENT_KIND_ENUM).into_iter().flatten() {
        let Some(kind) = markers::value(variant.docstring.as_deref(), markers::KIND) else {
            bail!(
                "{EVENT_KIND_ENUM}.{}: event kind has no #[sdk(kind = \"...\")]",
                variant.name
            );
        };
        kinds.insert(variant.name.clone(), kind.to_owned());
    }
    // The pure module has neither enum.
    let Some(events) = variants(EVENT_ENUM) else {
        return Ok(kinds);
    };
    let emitted = events
        .iter()
        .map(|variant| variant.name.as_str())
        .collect::<BTreeSet<_>>();
    let missing = kinds
        .keys()
        .map(String::as_str)
        .filter(|name| !emitted.contains(name))
        .collect::<Vec<_>>();
    let extra = emitted
        .iter()
        .copied()
        .filter(|name| !kinds.contains_key(*name))
        .collect::<Vec<_>>();
    if !missing.is_empty() || !extra.is_empty() {
        bail!(
            "{EVENT_KIND_ENUM} and {EVENT_ENUM} must have the same variants; \
             {EVENT_ENUM} lacks [{}] and {EVENT_KIND_ENUM} lacks [{}]",
            missing.join(", "),
            extra.join(", ")
        );
    }
    Ok(kinds)
}

#[cfg(test)]
mod tests {
    use uniffi_meta::FnParamMetadata;

    use super::*;
    use crate::test_metadata::{
        enum_type, enumeration, field, optional, record, record_type, sequence, variant,
    };

    fn kind(kind: &str) -> String {
        format!("@xmtp-kind={kind}")
    }

    fn event_enums(variants: &[(&str, &str)]) -> [Metadata; 2] {
        [
            enumeration(
                "EventKind",
                variants
                    .iter()
                    .map(|(name, public)| variant(name, Some(&kind(public)), vec![]))
                    .collect(),
            ),
            enumeration(
                "ClientEvent",
                variants
                    .iter()
                    .map(|(name, _)| {
                        variant(
                            name,
                            None,
                            vec![field(&name.to_snake_case(), record_type(name), None)],
                        )
                    })
                    .collect(),
            ),
        ]
    }

    // The payload records, nested payload records, and the filter keep their
    // Rust field names; any other record does not. A new payload record is
    // found through the event enum, so it needs no generator change.
    #[xmtp_common::test(unwrap_try = true)]
    fn event_records_are_the_closure_of_the_event_enum_and_filter() {
        let [event_kind, _] = event_enums(&[
            ("MessageReceived", "message.received"),
            ("BrandNew", "brand.new"),
        ]);
        let items = [
            event_kind,
            enumeration(
                "ClientEvent",
                vec![
                    variant(
                        "MessageReceived",
                        None,
                        vec![field(
                            "message_received",
                            record_type("MessageReceived"),
                            None,
                        )],
                    ),
                    variant(
                        "BrandNew",
                        None,
                        vec![field(
                            "brand_new",
                            optional(record_type("BrandNewPayload")),
                            None,
                        )],
                    ),
                ],
            ),
            record(
                "MessageReceived",
                vec![field(
                    "content_type",
                    optional(record_type("EventContentTypeId")),
                    None,
                )],
            ),
            record(
                "EventContentTypeId",
                vec![field("type_id", Type::String, None)],
            ),
            record(
                "BrandNewPayload",
                vec![
                    field("group_id", Type::Bytes, None),
                    field("cause", enum_type("BrandNewCause"), None),
                ],
            ),
            enumeration(
                "BrandNewCause",
                vec![variant(
                    "Failed",
                    None,
                    vec![field("detail", record_type("CauseDetail"), None)],
                )],
            ),
            record(
                "CauseDetail",
                vec![field("retry_after", Type::UInt64, None)],
            ),
            record(
                "EventFilter",
                vec![field(
                    "content_types",
                    sequence(record_type("EventContentTypeId")),
                    None,
                )],
            ),
            record(
                "ClientOptions",
                vec![field("app_version", Type::String, None)],
            ),
        ];
        let items = items.iter().collect::<Vec<_>>();
        let (records, enums) = event_types(&items)?;
        assert_eq!(
            records,
            BTreeSet::from([
                "BrandNewPayload".to_owned(),
                "CauseDetail".to_owned(),
                "EventContentTypeId".to_owned(),
                "EventFilter".to_owned(),
                "MessageReceived".to_owned(),
            ])
        );
        assert_eq!(enums, BTreeSet::from(["BrandNewCause".to_owned()]));
        let mut code = String::new();
        let Metadata::Record(payload) = items[4] else {
            unreachable!()
        };
        super::super::values::record(&mut code, payload, &Events::new(&items)?)?;
        assert!(code.contains("readonly group_id: Uint8Array;"));
    }

    // A record that an event and another call both reach would need two
    // spellings of its fields. Calls that take the filter or return the event
    // enum are the event API itself.
    #[xmtp_common::test(unwrap_try = true)]
    fn event_record_shared_with_another_call_stops_generation() {
        let call = |inputs: Vec<Type>, output: Type| {
            Metadata::Method(uniffi_meta::MethodMetadata {
                module_path: "test".into(),
                self_name: "Client".into(),
                name: "call".into(),
                orig_name: None,
                is_async: true,
                inputs: inputs
                    .into_iter()
                    .map(|ty| FnParamMetadata::simple("value", ty))
                    .collect(),
                return_type: Some(output),
                throws: None,
                takes_self_by_arc: true,
                checksum: None,
                docstring: None,
            })
        };
        let mut items = vec![
            enumeration(
                "ClientEvent",
                vec![variant(
                    "GroupJoined",
                    None,
                    vec![field("group_joined", record_type("GroupRef"), None)],
                )],
            ),
            record("GroupRef", vec![field("group_id", Type::Bytes, None)]),
            record("EventFilter", vec![field("group_ids", Type::Bytes, None)]),
            call(vec![record_type("EventFilter")], enum_type("ClientEvent")),
        ];
        assert_eq!(
            event_types(&items.iter().collect::<Vec<_>>())?.0,
            BTreeSet::from(["EventFilter".to_owned(), "GroupRef".to_owned()])
        );
        items.push(call(vec![], sequence(record_type("GroupRef"))));
        let error = event_types(&items.iter().collect::<Vec<_>>()).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("GroupRef: an event payload and another call share")
        );

        // An enum keeps one set of values. Shared one-word values read the
        // same either way, and so do marked ones.
        let state = |variants: Vec<VariantMetadata>| {
            vec![
                enumeration(
                    "ClientEvent",
                    vec![variant(
                        "StateChanged",
                        None,
                        vec![field("state", enum_type("State"), None)],
                    )],
                ),
                enumeration("State", variants),
                call(vec![], enum_type("State")),
            ]
        };
        let items = state(vec![
            variant("Connected", None, vec![]),
            variant("NotConnected", Some("@xmtp-kind=offline"), vec![]),
        ]);
        let (_, enums) = event_types(&items.iter().collect::<Vec<_>>())?;
        assert_eq!(enums, BTreeSet::from(["State".to_owned()]));
        let items = state(vec![
            variant("Connected", None, vec![]),
            variant("NotConnected", None, vec![]),
        ]);
        let error = event_types(&items.iter().collect::<Vec<_>>()).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("State.NotConnected: an event payload and another call share"),
            "{error}"
        );
    }

    // The spec spells event enum values in snake_case, so a cause enum needs
    // no marker: the derived strings are the ones the spec lists. An enum
    // that no event carries keeps camelCase.
    #[xmtp_common::test(unwrap_try = true)]
    fn event_enums_use_snake_case_and_others_camel_case() {
        let [event_kind, client_event] = event_enums(&[("MessageDeleted", "message.deleted")]);
        let cause = |name: &str, variants: &[&str]| {
            enumeration(
                name,
                variants
                    .iter()
                    .map(|name| variant(name, None, vec![]))
                    .collect(),
            )
        };
        let items = [
            event_kind,
            client_event,
            record(
                "MessageDeleted",
                vec![
                    field("cause", enum_type("DeletionCause"), None),
                    field("rejected", optional(enum_type("RejectionCause")), None),
                    field("origin", enum_type("JoinOrigin"), None),
                ],
            ),
            cause("DeletionCause", &["Deleted", "DeletedLocally"]),
            cause("RejectionCause", &["BackendMismatch", "VersionTooOld"]),
            cause("JoinOrigin", &["Created", "Welcomed"]),
            cause("SortDirection", &["Ascending", "NewestFirst"]),
        ];
        let items = items.iter().collect::<Vec<_>>();
        let events = Events::new(&items)?;
        let strings = |name: &str| {
            items
                .iter()
                .find_map(|item| match item {
                    Metadata::Enum(value) if value.name == name => Some(
                        value
                            .variants
                            .iter()
                            .map(|variant| events.variant_kind(name, variant).unwrap())
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(strings("DeletionCause"), ["deleted", "deleted_locally"]);
        assert_eq!(
            strings("RejectionCause"),
            ["backend_mismatch", "version_too_old"]
        );
        assert_eq!(strings("JoinOrigin"), ["created", "welcomed"]);
        assert_eq!(strings("SortDirection"), ["ascending", "newestFirst"]);
        assert_eq!(strings("EventKind"), ["message.deleted"]);
        assert_eq!(strings("ClientEvent"), ["message.deleted"]);
        // A kind marker on any other enum names its strings.
        assert_eq!(
            events.variant_kind(
                "SortDirection",
                &variant("NewestFirst", Some("@xmtp-kind=newest"), vec![])
            )?,
            "newest"
        );
    }

    // EventKind writes each kind once; ClientEvent takes it by variant name.
    #[xmtp_common::test(unwrap_try = true)]
    fn client_event_kinds_come_from_event_kind_by_variant_name() {
        let items = event_enums(&[
            ("HmacKeysUpdated", "hmac_keys.updated"),
            ("Lagged", "lagged"),
        ]);
        let events = Events::new(&items.iter().collect::<Vec<_>>())?;
        assert_eq!(
            events.variant_kind("ClientEvent", &variant("HmacKeysUpdated", None, vec![]))?,
            "hmac_keys.updated"
        );
        let error = events
            .variant_kind(
                "ClientEvent",
                &variant("Lagged", Some("@xmtp-kind=x"), vec![]),
            )
            .unwrap_err();
        assert!(error.to_string().contains("takes its kinds from EventKind"));

        let [event_kind, mut client_event] = event_enums(&[
            ("HmacKeysUpdated", "hmac_keys.updated"),
            ("Lagged", "lagged"),
        ]);
        let Metadata::Enum(value) = &mut client_event else {
            unreachable!()
        };
        value.variants[1].name = "Dropped".into();
        let error = Events::new(&[&event_kind, &client_event])
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.ends_with("ClientEvent lacks [Lagged] and EventKind lacks [Dropped]"),
            "{error}"
        );

        // An emitted event without a filter kind stops generation too.
        let [event_kind, _] = event_enums(&[("Lagged", "lagged")]);
        let [_, client_event] = event_enums(&[("Lagged", "lagged"), ("Dropped", "dropped")]);
        let error = Events::new(&[&event_kind, &client_event])
            .err()
            .unwrap()
            .to_string();
        assert!(error.ends_with("ClientEvent lacks [] and EventKind lacks [Dropped]"));

        let unmarked = enumeration("EventKind", vec![variant("Lagged", None, vec![])]);
        let error = Events::new(&[&unmarked]).err().unwrap().to_string();
        assert!(error.contains("EventKind.Lagged: event kind has no #[sdk(kind"));
        // No event enums at all (the pure module) is fine.
        Events::new(&[])?;
    }
}
