//! Oracle while the xmtp OpenMLS fork is pinned: the legacy decoder that read
//! the group context with the fork's types and bincode.
//!
//! These tests check the std-only reader against that decoder, and check that
//! `fixtures/legacy-context-corpus.json` was built with the fork's types.
//! Remove this file and the bincode and openmls dev-dependencies when the pin
//! moves to upstream OpenMLS. The corpus test in `metadata.rs` keeps the
//! behavior.
use super::tests::{Case, corpus, summary};
use super::{
    COMPONENT_REGISTRY, CREATOR, ImmutableMetadataSave, MAX_CONTEXT_BYTES,
    MUTABLE_METADATA_EXTENSION, MutableMetadataSave, decode, decode_legacy_immutable,
    decode_legacy_mutable,
};
use bincode::Options;
use openmls::prelude::{
    AppDataDictionaryExtension, ApplicationIdExtension, CredentialType, Extension, ExtensionType,
    ExternalPubExtension, GroupContext, GroupEpoch, GroupId, HpkePublicKey, LastResortExtension,
    Metadata, ProposalType, ProtocolVersion, RequiredCapabilitiesExtension, UnknownExtension,
};
use prost::Message;
use serde_json::{Value, json};
use tls_codec::{Deserialize, Serialize};
use xmtp_mls_common::{
    group_mutable_metadata::{GroupMutableMetadata, merge_dict_into_mutable_metadata_lossy},
    inbox_id::InboxId,
};
use xmtp_proto::xmtp::mls::message_contents::{GroupMetadataV1, GroupMutableMetadataV1, Inboxes};

const APPDATA_CONTEXT: &[u8] = include_bytes!("../../fixtures/appdata-context.bincode");
/// The offset of the u32 ciphersuite tag in a stored context.
const CIPHERSUITE_TAG_OFFSET: usize = 4;

fn openmls_context(bytes: &[u8]) -> Option<GroupContext> {
    if bytes.len() > MAX_CONTEXT_BYTES {
        return None;
    }
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .allow_trailing_bytes()
        .with_limit(MAX_CONTEXT_BYTES as u64)
        .deserialize(bytes)
        .ok()
}

/// The decoder `metadata::decode` used before the std-only reader.
fn openmls_decode(bytes: &[u8]) -> (Option<ImmutableMetadataSave>, Option<MutableMetadataSave>) {
    let Some(context) = openmls_context(bytes) else {
        return (None, None);
    };
    let extensions = context.extensions();
    let mut immutable = extensions
        .immutable_metadata()
        .and_then(|e| decode_legacy_immutable(e.metadata().as_slice()));
    let mut mutable = extensions.iter().find_map(|ext| {
        if let Extension::Unknown(MUTABLE_METADATA_EXTENSION, UnknownExtension(bytes)) = ext {
            decode_legacy_mutable(bytes)
        } else {
            None
        }
    });
    if let Some(extension) = extensions.app_data_dictionary() {
        let dict = extension.dictionary();
        if dict.get(&COMPONENT_REGISTRY).is_some() {
            immutable = dict
                .get(&CREATOR)
                .and_then(|bytes| InboxId::tls_deserialize_exact(bytes).ok())
                .map(|id| ImmutableMetadataSave {
                    creator_inbox_id: hex::encode(id.as_bytes()),
                })
                .or(immutable);
            let mut result = GroupMutableMetadata::new(Default::default(), vec![], vec![]);
            merge_dict_into_mutable_metadata_lossy(&mut result, extensions);
            mutable = Some(MutableMetadataSave {
                attributes: result.take_attributes(),
                admin_list: std::mem::take(&mut result.admin_list),
                super_admin_list: std::mem::take(&mut result.super_admin_list),
            });
        }
    }
    (immutable, mutable)
}

/// True when every extension has a type the std-only reader reads.
fn readable(context: &GroupContext) -> bool {
    context.extensions().iter().all(|extension| {
        matches!(
            extension,
            Extension::RequiredCapabilities(_)
                | Extension::ImmutableMetadata(_)
                | Extension::Unknown(..)
                | Extension::AppDataDictionary(_)
        )
    })
}

/// What the std-only reader must return: the legacy decoder's result for
/// contexts it can read, and "metadata unavailable" for everything else.
fn expected(bytes: &[u8]) -> (Option<ImmutableMetadataSave>, Option<MutableMetadataSave>) {
    match openmls_context(bytes) {
        Some(context) if readable(&context) => openmls_decode(bytes),
        _ => (None, None),
    }
}

fn check(bytes: &[u8], label: &str, failures: &mut Vec<String>) {
    if decode(bytes) != expected(bytes) {
        failures.push(label.to_owned());
    }
}

fn context_header(protocol_version: ProtocolVersion, tree_hash: &[u8]) -> Vec<u8> {
    let base: GroupContext = bincode::deserialize(APPDATA_CONTEXT).unwrap();
    bincode::serialize(&(
        protocol_version,
        base.ciphersuite(),
        GroupId::from_slice(&[0xa0; 4]),
        GroupEpoch::from(3),
        tree_hash,
        &[0xc0u8; 2][..],
    ))
    .unwrap()
}

fn context(header: &[u8], extensions: &[Extension]) -> Vec<u8> {
    let mut bytes = header.to_vec();
    bytes.extend(bincode::serialize(extensions).unwrap());
    bytes
}

fn immutable_metadata(creator: &str) -> Extension {
    Extension::ImmutableMetadata(Metadata::new(
        GroupMetadataV1 {
            creator_inbox_id: creator.repeat(32),
            ..Default::default()
        }
        .encode_to_vec(),
    ))
}

fn app_data(edit: impl FnOnce(&mut openmls::prelude::AppDataDictionary)) -> Extension {
    let base: GroupContext = bincode::deserialize(APPDATA_CONTEXT).unwrap();
    let mut dictionary = base
        .extensions()
        .app_data_dictionary()
        .unwrap()
        .dictionary()
        .clone();
    edit(&mut dictionary);
    Extension::AppDataDictionary(AppDataDictionaryExtension::new(dictionary))
}

/// The extensions a legacy XMTP group context could hold, by name.
fn atoms() -> Vec<(&'static str, Extension)> {
    let field = |tag: u32, bytes: &[u8]| {
        let mut encoded = vec![];
        prost::encoding::encode_key(
            tag,
            prost::encoding::WireType::LengthDelimited,
            &mut encoded,
        );
        prost::encoding::encode_varint(bytes.len() as u64, &mut encoded);
        encoded.extend_from_slice(bytes);
        encoded
    };
    let nested_creator = GroupMetadataV1 {
        creator_inbox_id: "04".repeat(32),
        ..Default::default()
    }
    .encode_to_vec();
    vec![
        (
            "required-capabilities",
            Extension::RequiredCapabilities(RequiredCapabilitiesExtension::new(
                &[
                    ExtensionType::ApplicationId,
                    ExtensionType::RatchetTree,
                    ExtensionType::RequiredCapabilities,
                    ExtensionType::ExternalPub,
                    ExtensionType::ExternalSenders,
                    ExtensionType::LastResort,
                    ExtensionType::ImmutableMetadata,
                    ExtensionType::Unknown(MUTABLE_METADATA_EXTENSION),
                    ExtensionType::Grease(0x0a0a),
                    ExtensionType::AppDataDictionary,
                ],
                &[
                    ProposalType::Add,
                    ProposalType::Update,
                    ProposalType::Remove,
                    ProposalType::PreSharedKey,
                    ProposalType::Reinit,
                    ProposalType::ExternalInit,
                    ProposalType::GroupContextExtensions,
                    ProposalType::SelfRemove,
                    ProposalType::Custom(0xff01),
                    ProposalType::Grease(0x1a1a),
                    ProposalType::AppEphemeral,
                    ProposalType::AppDataUpdate,
                ],
                &[
                    CredentialType::Basic,
                    CredentialType::X509,
                    CredentialType::Other(0xf001),
                    CredentialType::Grease(0x2a2a),
                ],
            )),
        ),
        (
            "required-capabilities-empty",
            Extension::RequiredCapabilities(RequiredCapabilitiesExtension::default()),
        ),
        ("immutable", immutable_metadata("03")),
        (
            "immutable-malformed",
            Extension::ImmutableMetadata(Metadata::new(vec![0xff])),
        ),
        (
            "immutable-nested-creator",
            Extension::ImmutableMetadata(Metadata::new(
                [field(3, &nested_creator), field(2, &[0xff])].concat(),
            )),
        ),
        (
            "mutable",
            Extension::Unknown(
                MUTABLE_METADATA_EXTENSION,
                UnknownExtension(
                    GroupMutableMetadataV1 {
                        attributes: [("group_name".into(), "Legacy Group".into())].into(),
                        admin_list: Some(Inboxes {
                            inbox_ids: vec!["05".repeat(32)],
                        }),
                        super_admin_list: Some(Inboxes {
                            inbox_ids: vec!["06".repeat(32)],
                        }),
                    }
                    .encode_to_vec(),
                ),
            ),
        ),
        (
            "mutable-malformed",
            Extension::Unknown(
                MUTABLE_METADATA_EXTENSION,
                UnknownExtension(vec![0x0a, 0xff]),
            ),
        ),
        (
            "unknown-other",
            Extension::Unknown(0x1234, UnknownExtension(vec![1, 2, 3])),
        ),
        ("app-data", app_data(|_| {})),
        (
            "app-data-without-registry",
            app_data(|dictionary| {
                dictionary.remove(&COMPONENT_REGISTRY);
            }),
        ),
        (
            "app-data-creator-malformed",
            app_data(|dictionary| {
                dictionary.insert(CREATOR, vec![0xff]);
            }),
        ),
        (
            "app-data-empty",
            Extension::AppDataDictionary(AppDataDictionaryExtension::default()),
        ),
    ]
}

/// Extension types XMTP never stored in a group context.
fn other_types() -> Vec<(&'static str, Extension)> {
    vec![
        (
            "application-id",
            Extension::ApplicationId(ApplicationIdExtension::new(b"id")),
        ),
        (
            "last-resort",
            Extension::LastResort(LastResortExtension::new()),
        ),
        ("external-senders", Extension::ExternalSenders(vec![])),
        (
            "external-pub",
            Extension::ExternalPub(ExternalPubExtension::new(HpkePublicKey::from(vec![
                1u8;
                32
            ]))),
        ),
    ]
}

/// Build the corpus with the fork's types. The order and bytes are
/// deterministic, so the saved corpus must equal this list.
fn build_corpus() -> Vec<(String, Vec<u8>)> {
    let header = context_header(ProtocolVersion::Mls10, &[0xb0; 2]);
    let atoms = atoms();
    let mut cases = vec![("no-extensions".to_owned(), context(&header, &[]))];
    for (name, atom) in &atoms {
        cases.push((
            (*name).to_owned(),
            context(&header, std::slice::from_ref(atom)),
        ));
    }
    // Pairs cover first-wins lookups, repeated types, and order.
    let paired: Vec<_> = atoms
        .iter()
        .filter(|(name, _)| !name.ends_with("-empty") && *name != "unknown-other")
        .collect();
    for (first, a) in &paired {
        for (second, b) in &paired {
            cases.push((
                format!("{first}+{second}"),
                context(&header, &[a.clone(), b.clone()]),
            ));
        }
    }
    let atom = |wanted: &str| {
        atoms
            .iter()
            .find(|(name, _)| *name == wanted)
            .unwrap()
            .1
            .clone()
    };
    let everything = context(
        &header,
        &[
            atom("required-capabilities"),
            atom("unknown-other"),
            atom("immutable"),
            atom("mutable"),
            atom("app-data"),
        ],
    );
    cases.push(("everything".to_owned(), everything.clone()));
    for length in [0, 4, 8, 20, everything.len() / 2, everything.len() - 1] {
        cases.push((
            format!("everything-truncated-{length}"),
            everything[..length].to_vec(),
        ));
    }
    let other_version = context_header(ProtocolVersion::Other(2), &[0xb0; 2]);
    cases.push((
        "protocol-version-other".to_owned(),
        context(&other_version, &[atom("immutable"), atom("mutable")]),
    ));
    // Serde stores the ciphersuite as its variant index. Index 17 is past the
    // fork's last variant.
    let immutable = context(&header, &[atom("immutable")]);
    for index in 0u32..=17 {
        let mut bytes = immutable.clone();
        bytes[CIPHERSUITE_TAG_OFFSET..CIPHERSUITE_TAG_OFFSET + 4]
            .copy_from_slice(&index.to_le_bytes());
        cases.push((format!("ciphersuite-{index}"), bytes));
    }
    // Byte edits for shapes the fork's types cannot build. The context ends
    // with the proposal tag and an empty credential list; tag 7 is unused.
    let mut unused_proposal_tag = context(
        &header,
        &[
            atom("immutable"),
            Extension::RequiredCapabilities(RequiredCapabilitiesExtension::new(
                &[],
                &[ProposalType::GroupContextExtensions],
                &[],
            )),
        ],
    );
    let tag = unused_proposal_tag.len() - 12;
    unused_proposal_tag[tag..tag + 4].copy_from_slice(&7u32.to_le_bytes());
    cases.push(("proposal-tag-7".to_owned(), unused_proposal_tag));
    // A dictionary whose last key repeats the creator key. The map decoder
    // keeps the last value.
    let later_creator = InboxId::from_hex(&"07".repeat(32))
        .unwrap()
        .tls_serialize_detached()
        .unwrap();
    let mut repeated_key = context(
        &header,
        &[app_data(|dictionary| {
            dictionary.insert(CREATOR + 1, later_creator.clone());
        })],
    );
    let key = repeated_key.len() - later_creator.len() - 8 - 4;
    assert_eq!(repeated_key[key..key + 4], [0xff, 0xbf, 0xff, 0xbf]);
    repeated_key[key..key + 4].copy_from_slice(&[0xfe, 0xbf, 0xfe, 0xbf]);
    cases.push(("app-data-repeated-creator-key".to_owned(), repeated_key));
    for (name, other) in other_types() {
        cases.push((
            format!("{name}+immutable"),
            context(&header, &[other, atom("immutable")]),
        ));
    }
    // The cases of the metadata unit tests in metadata.rs.
    let base: GroupContext = bincode::deserialize(APPDATA_CONTEXT).unwrap();
    let base_header = bincode::serialize(&(
        base.protocol_version(),
        base.ciphersuite(),
        base.group_id(),
        base.epoch(),
        base.tree_hash(),
        base.confirmed_transcript_hash(),
    ))
    .unwrap();
    let valid_creator = base
        .extensions()
        .app_data_dictionary()
        .unwrap()
        .dictionary()
        .get(&CREATOR)
        .unwrap()
        .to_vec();
    for (name, creator) in [
        ("appdata-creator-absent", None),
        ("appdata-creator-malformed", Some(vec![0xff])),
        ("appdata-creator-valid", Some(valid_creator)),
    ] {
        let app_data = app_data(|dictionary| {
            dictionary.remove(&CREATOR);
            if let Some(bytes) = creator {
                dictionary.insert(CREATOR, bytes);
            }
        });
        cases.push((
            name.to_owned(),
            context(&base_header, &[app_data, immutable_metadata("02")]),
        ));
    }
    let below_limit = bincode::serialize(&(
        base.protocol_version(),
        base.ciphersuite(),
        base.group_id(),
        base.epoch(),
        vec![0u8; 1024],
        base.confirmed_transcript_hash(),
        base.extensions(),
    ))
    .unwrap();
    cases.push(("appdata-tree-hash-1024".to_owned(), below_limit));
    for length in [1u64 << 40, u64::MAX] {
        let mut bytes = bincode::serialize(&(
            base.protocol_version(),
            base.ciphersuite(),
            base.group_id(),
            base.epoch(),
        ))
        .unwrap();
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.push(0);
        cases.push((format!("declared-length-{length:#x}"), bytes));
    }
    cases
}

fn corpus_json(cases: &[(String, Vec<u8>)]) -> Value {
    json!({
        "description": "Legacy GroupContext records written with the xmtp OpenMLS fork 3fabbf23 and bincode 1.3, and the metadata each one decodes to. src/metadata/oracle.rs builds them.",
        "cases": cases
            .iter()
            .map(|(name, bytes)| json!({
                "name": name,
                "context": hex::encode(bytes),
                "expected": summary(&expected(bytes)),
            }))
            .collect::<Vec<_>>(),
    })
}

// verifies: MIG-005
#[xmtp_common::test(unwrap_try = true)]
fn corpus_fixture_was_built_with_the_pinned_types() {
    let built = build_corpus();
    let saved = corpus();
    let names = |cases: &[(String, Vec<u8>)]| cases.iter().map(|c| c.0.clone()).collect::<Vec<_>>();
    assert_eq!(
        saved
            .iter()
            .map(|case| case.name.clone())
            .collect::<Vec<_>>(),
        names(&built)
    );
    for (
        Case {
            name,
            context,
            expected: saved_expected,
        },
        (_, bytes),
    ) in saved.iter().zip(&built)
    {
        assert_eq!(
            context, bytes,
            "{name}: corpus bytes differ from the pinned types"
        );
        assert_eq!(
            *saved_expected,
            summary(&expected(bytes)),
            "{name}: recorded metadata differs from the legacy decoder"
        );
    }
    // The corpus exercises both sides of the comparison.
    let readable_cases = built
        .iter()
        .filter(|(_, bytes)| openmls_context(bytes).is_some_and(|context| readable(&context)))
        .count();
    assert!(readable_cases > 100, "only {readable_cases} readable cases");
    assert!(
        built.iter().any(|(_, bytes)| {
            openmls_context(bytes).is_some_and(|context| !readable(&context))
        })
    );
}

// verifies: MIG-005
#[xmtp_common::test(unwrap_try = true)]
fn reader_matches_the_legacy_decoder_on_every_corpus_case() {
    let mut failures = vec![];
    for case in corpus() {
        check(&case.context, &case.name, &mut failures);
    }
    assert!(failures.is_empty(), "reader differs on {failures:?}");
}

// verifies: MIG-005
#[xmtp_common::test(unwrap_try = true)]
fn reader_matches_the_legacy_decoder_on_truncated_and_mutated_contexts() {
    let mut failures = vec![];
    for case in corpus() {
        for length in 0..case.context.len() {
            check(
                &case.context[..length],
                &format!("{} truncated to {length}", case.name),
                &mut failures,
            );
        }
    }
    // Each byte of a context with every readable extension type, changed three ways.
    let everything = corpus()
        .into_iter()
        .find(|case| case.name == "everything")
        .unwrap()
        .context;
    for (index, &byte) in everything.iter().enumerate() {
        for value in [0x00, 0xff, byte ^ 0x01] {
            let mut bytes = everything.clone();
            bytes[index] = value;
            check(
                &bytes,
                &format!("everything[{index}] = {value:#04x}"),
                &mut failures,
            );
        }
    }
    assert!(failures.is_empty(), "reader differs on {failures:?}");
}

/// Every group context in the SQLite fixtures, read from temporary copies.
/// `early.db3` has no OpenMLS rows, and the encrypted fixtures need a key.
#[cfg(not(target_arch = "wasm32"))]
fn fixture_database_contexts() -> Vec<(String, Vec<u8>)> {
    use diesel::{Connection, QueryableByName, RunQueryDsl, SqliteConnection, sql_types::Binary};

    #[derive(QueryableByName)]
    struct Row {
        #[diesel(sql_type = Binary)]
        key_bytes: Vec<u8>,
        #[diesel(sql_type = Binary)]
        value_bytes: Vec<u8>,
    }

    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let mut contexts = vec![];
    for name in [
        "metadata-seed.db3",
        "appdata-seed.db3",
        "stable.db3",
        "consent-states.db3",
        "mobile-4.10.db3",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let copy = directory.path().join(name);
        std::fs::copy(fixtures.join(name), &copy).unwrap();
        let mut conn = SqliteConnection::establish(copy.to_str().unwrap()).unwrap();
        let rows: Vec<Row> =
            diesel::sql_query("SELECT key_bytes, value_bytes FROM openmls_key_value")
                .load(&mut conn)
                .unwrap();
        let before = contexts.len();
        for (index, row) in rows.into_iter().enumerate() {
            if row.key_bytes.starts_with(b"GroupContextGroupContext") {
                contexts.push((format!("{name} row {index}"), row.value_bytes));
            }
        }
        assert!(contexts.len() > before, "{name} has no group context");
    }
    contexts
}

// verifies: MIG-005
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
fn reader_matches_the_legacy_decoder_on_fixture_databases() {
    let mut failures = vec![];
    for (label, bytes) in fixture_database_contexts() {
        let context = openmls_context(&bytes).expect("fixture context decodes");
        assert!(
            readable(&context),
            "{label} has an extension type the reader skips"
        );
        assert_ne!(decode(&bytes), (None, None), "{label} has no metadata");
        check(&bytes, &label, &mut failures);
        for length in 0..bytes.len() {
            check(
                &bytes[..length],
                &format!("{label} truncated to {length}"),
                &mut failures,
            );
        }
    }
    assert!(failures.is_empty(), "reader differs on {failures:?}");
}

// verifies: MIG-005
#[xmtp_common::test(unwrap_try = true)]
fn over_budget_context_is_valid_but_unavailable() {
    let below_limit = corpus()
        .into_iter()
        .find(|case| case.name == "appdata-tree-hash-1024")
        .unwrap()
        .context;
    let base: GroupContext = bincode::deserialize(&below_limit)?;
    let bytes = bincode::serialize(&(
        base.protocol_version(),
        base.ciphersuite(),
        base.group_id(),
        base.epoch(),
        vec![0u8; MAX_CONTEXT_BYTES],
        base.confirmed_transcript_hash(),
        base.extensions(),
    ))?;
    assert!(bincode::deserialize::<GroupContext>(&bytes).is_ok());
    assert_eq!(decode(&bytes), (None, None));
}

/// Rewrite the corpus fixture from the fork's types. Run it only on purpose:
/// `just test workspace -p xmtp_legacy_migration write_corpus_fixture --run-ignored only`.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
#[ignore = "rewrites fixtures/legacy-context-corpus.json"]
fn write_corpus_fixture() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/legacy-context-corpus.json");
    let mut text = serde_json::to_string_pretty(&corpus_json(&build_corpus()))?;
    text.push('\n');
    std::fs::write(path, text)?;
}
