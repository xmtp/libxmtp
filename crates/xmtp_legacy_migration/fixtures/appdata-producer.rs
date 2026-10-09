use openmls::{
    extensions::{
        AppDataDictionary, AppDataDictionaryExtension, Extension, ExtensionType, Extensions,
        Metadata, UnknownExtension,
    },
    prelude::*,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::{
    OpenMlsProvider,
    storage::{CURRENT_VERSION, StorageProvider},
};
use prost::Message;
use std::{collections::HashMap, fs};
use tls_codec::Serialize;
use xmtp_db::{
    ConnectionExt, DefaultStore, XmtpOpenMlsProvider, database::NativeDb,
    sql_key_store::SqlKeyStore,
};
use xmtp_mls_common::{
    app_data::component_id::ComponentId,
    group_metadata::{DmMembers, GroupMetadata, extract_group_metadata},
    group_mutable_metadata::{
        GroupMutableMetadata, extract_legacy_group_mutable_metadata_from_extensions,
        merge_dict_into_mutable_metadata_lossy,
    },
    inbox_id::InboxId,
};
use xmtp_proto::xmtp::identity::MlsCredential;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--decode") {
        let input = std::env::args().nth(2).expect("fixture directory");
        let expected: serde_json::Value =
            serde_json::from_slice(&fs::read(format!("{input}/normalized.json"))?)?;
        let bytes = fs::read(format!("{input}/group-context.bincode"))?;
        let context: GroupContext = bincode::deserialize(&bytes)?;
        let immutable = extract_group_metadata(context.extensions())?;
        let mut mutable =
            extract_legacy_group_mutable_metadata_from_extensions(context.extensions())?;
        if std::env::args().any(|arg| arg == "--broken") {
            mutable.attributes.remove("group_name");
        }
        let actual = serde_json::json!({"creator_inbox_id":immutable.creator_inbox_id,"dm_members":immutable.dm_members,"conversation_type":format!("{:?}", immutable.conversation_type),"attributes":mutable.attributes,"admin_list":mutable.admin_list,"super_admin_list":mutable.super_admin_list});
        for field in [
            "creator_inbox_id",
            "dm_members",
            "conversation_type",
            "attributes",
            "admin_list",
            "super_admin_list",
        ] {
            assert_eq!(
                actual[field], expected[field],
                "cross-revision decode field {field}"
            );
        }
        println!("{}", serde_json::to_string_pretty(&actual)?);
        return Ok(());
    }
    let out = std::env::args().nth(1).expect("output directory");
    let broken = std::env::args().any(|arg| arg == "--broken");
    fs::create_dir_all(&out)?;
    let path = format!("{out}/metadata.sqlite");
    if std::path::Path::new(&path).exists() {
        fs::remove_file(&path)?;
    }
    let db = NativeDb::builder()
        .persistent(path.clone())
        .build_unencrypted()?;
    let store = DefaultStore::new(db)?;
    let provider = XmtpOpenMlsProvider::new(SqlKeyStore::new(store.conn()));
    let alice = hex::encode([1u8; 32]);
    let bob = hex::encode([2u8; 32]);
    let immutable = GroupMetadata::new(
        xmtp_db::group::ConversationType::Dm,
        alice.clone(),
        Some(DmMembers {
            member_one_inbox_id: alice.clone(),
            member_two_inbox_id: bob.clone(),
        }),
        None,
    );
    let mutable = GroupMutableMetadata::new(
        HashMap::from([
            ("group_name".into(), "Migration DM".into()),
            ("description".into(), "Legacy fixture".into()),
            (
                "group_image_url_square".into(),
                "https://example.test/image.png".into(),
            ),
            ("app_data".into(), "fixture-app-data".into()),
            (
                "message_disappear_from_ns".into(),
                "1700000000000000000".into(),
            ),
            ("message_disappear_in_ns".into(), "60000000000".into()),
        ]),
        vec![bob.clone()],
        vec![alice.clone()],
    );
    let immutable_bytes: Vec<u8> = immutable.clone().try_into()?;
    let mutable_bytes: Vec<u8> = mutable.clone().try_into()?;
    let extension_types = [
        ExtensionType::ImmutableMetadata,
        ExtensionType::Unknown(xmtp_configuration::MUTABLE_METADATA_EXTENSION_ID),
        ExtensionType::AppDataDictionary,
    ];
    let capabilities = Capabilities::new(None, None, Some(&extension_types), None, None);
    let extensions = Extensions::from_vec(vec![
        Extension::ImmutableMetadata(Metadata::new(immutable_bytes)),
        Extension::Unknown(
            xmtp_configuration::MUTABLE_METADATA_EXTENSION_ID,
            UnknownExtension(mutable_bytes),
        ),
    ])?;
    let suite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;
    let signer = SignatureKeyPair::new(suite.signature_algorithm())?;
    let credential = CredentialWithKey {
        credential: BasicCredential::new(
            MlsCredential {
                inbox_id: alice.clone(),
            }
            .encode_to_vec(),
        )
        .into(),
        signature_key: signer.to_public_vec().into(),
    };
    let config = MlsGroupCreateConfig::builder()
        .ciphersuite(suite)
        .capabilities(capabilities.clone())
        .with_group_context_extensions(extensions)
        .build();
    let mut group = MlsGroup::new(&provider, &signer, &config, credential.clone())?;
    let other = openmls_rust_crypto::OpenMlsRustCrypto::default();
    let bob_signer = SignatureKeyPair::new(suite.signature_algorithm())?;
    let bob_credential = CredentialWithKey {
        credential: BasicCredential::new(
            MlsCredential {
                inbox_id: bob.clone(),
            }
            .encode_to_vec(),
        )
        .into(),
        signature_key: bob_signer.to_public_vec().into(),
    };
    let kp = KeyPackage::builder()
        .leaf_node_capabilities(capabilities.clone())
        .build(suite, &other, &bob_signer, bob_credential)?;
    group.add_members(&provider, &signer, &[kp.key_package().clone()])?;
    group.merge_pending_commit(&provider)?;
    let gid = group.group_id().clone();
    assert!(MlsGroup::load(provider.storage(), &gid)?.is_some());
    let getter_immutable = extract_group_metadata(group.extensions())?;
    let getter_mutable = GroupMutableMetadata::try_from(&group)?;
    let members: Vec<String> = group
        .members()
        .map(|m| MlsCredential::decode(m.credential.serialized_content()).map(|c| c.inbox_id))
        .collect::<Result<_, _>>()?;
    assert_eq!(members, vec![alice.clone(), bob.clone()]);

    let mut key = b"GroupContext".to_vec();
    key.extend(bincode::serialize(&gid)?);
    key.extend(CURRENT_VERSION.to_be_bytes());
    let mut outer_key = b"GroupContext".to_vec();
    outer_key.extend(key);
    outer_key.extend(CURRENT_VERSION.to_be_bytes());
    let key = outer_key;
    let conn = store.conn();
    #[derive(xmtp_db::diesel::QueryableByName)]
    struct Row {
        #[diesel(sql_type = xmtp_db::diesel::sql_types::Binary)]
        value_bytes: Vec<u8>,
    }
    use xmtp_db::diesel::RunQueryDsl;
    let bytes = conn
        .raw_query(|c| {
            xmtp_db::diesel::sql_query(
                "SELECT value_bytes FROM openmls_key_value WHERE key_bytes = ? AND version = ?",
            )
            .bind::<xmtp_db::diesel::sql_types::Binary, _>(&key)
            .bind::<xmtp_db::diesel::sql_types::Integer, _>(CURRENT_VERSION as i32)
            .get_result::<Row>(c)
        })?
        .value_bytes;
    fs::write(format!("{out}/group-context.bincode"), &bytes)?;
    fs::write(format!("{out}/group-context-key.bin"), &key)?;
    let decoded: GroupContext = bincode::deserialize(&bytes)?;
    let decoded_immutable = extract_group_metadata(decoded.extensions())?;
    let mut decoded_mutable =
        extract_legacy_group_mutable_metadata_from_extensions(decoded.extensions())?;
    if broken {
        decoded_mutable
            .attributes
            .insert("group_name".into(), "broken".into());
    }
    assert_eq!(decoded_immutable, getter_immutable);
    assert_eq!(decoded_mutable, getter_mutable);
    assert_eq!(decoded_immutable, immutable);
    assert_eq!(decoded_mutable, mutable);

    let mut dict = AppDataDictionary::new();
    let _ = dict.insert(ComponentId::COMPONENT_REGISTRY.as_u16(), vec![]);
    let _ = dict.insert(
        ComponentId::CONVERSATION_TYPE.as_u16(),
        1i32.to_be_bytes().to_vec(),
    );
    let _ = dict.insert(
        ComponentId::CREATOR_INBOX_ID.as_u16(),
        InboxId::from_bytes([1; 32]).tls_serialize_detached()?,
    );
    let _ = dict.insert(ComponentId::GROUP_NAME.as_u16(), b"AppData Group".to_vec());
    let _ = dict.insert(
        ComponentId::GROUP_DESCRIPTION.as_u16(),
        b"good description".to_vec(),
    );
    let _ = dict.insert(
        ComponentId::MESSAGE_DISAPPEAR_FROM_NS.as_u16(),
        vec![1, 2, 3],
    );
    let app_extensions = Extensions::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(dict),
    )])?;
    let app_immutable = extract_group_metadata(&app_extensions)?;
    assert_eq!(app_immutable.creator_inbox_id, alice);
    let mut app_mutable = GroupMutableMetadata::new(HashMap::new(), vec![], vec![]);
    let errors = merge_dict_into_mutable_metadata_lossy(&mut app_mutable, &app_extensions);
    assert_eq!(errors.len(), 1);
    assert_eq!(app_mutable.attributes["group_name"], "AppData Group");
    assert_eq!(app_mutable.attributes["description"], "good description");
    assert!(
        !app_mutable
            .attributes
            .contains_key("message_disappear_from_ns")
    );
    let app_config = MlsGroupCreateConfig::builder()
        .ciphersuite(suite)
        .capabilities(capabilities)
        .with_group_context_extensions(app_extensions)
        .build();
    let app_group = MlsGroup::new(&provider, &signer, &app_config, credential)?;
    let app_context: GroupContext = provider
        .storage()
        .group_context(app_group.group_id())?
        .expect("AppData context");
    assert_eq!(
        extract_group_metadata(app_context.extensions())?,
        app_immutable
    );
    let mut app_decoded = GroupMutableMetadata::new(HashMap::new(), vec![], vec![]);
    assert_eq!(
        merge_dict_into_mutable_metadata_lossy(&mut app_decoded, app_context.extensions()).len(),
        1
    );
    assert_eq!(app_decoded, app_mutable);

    let public_before = PublicGroup::load(provider.storage(), &gid)?.expect("public group");
    let tree_members: Vec<String> = public_before
        .members()
        .map(|m| MlsCredential::decode(m.credential.serialized_content()).map(|c| c.inbox_id))
        .collect::<Result<_, _>>()?;
    assert_eq!(tree_members, members);
    let full_path = format!("{out}/metadata-full.sqlite");
    if std::path::Path::new(&full_path).exists() {
        fs::remove_file(&full_path)?;
    }
    conn.raw_query(|c| {
        xmtp_db::diesel::sql_query("VACUUM INTO ?")
            .bind::<xmtp_db::diesel::sql_types::Text, _>(&full_path)
            .execute(c)
    })?;
    conn.raw_query(|c| xmtp_db::diesel::sql_query("DELETE FROM openmls_key_value WHERE substr(key_bytes, 1, 12) != ? AND substr(key_bytes, 1, 4) != ? AND substr(key_bytes, 1, 15) != ? AND substr(key_bytes, 1, 21) != ? AND substr(key_bytes, 1, 12) != ?")
        .bind::<xmtp_db::diesel::sql_types::Binary,_>(b"GroupContext".as_slice())
        .bind::<xmtp_db::diesel::sql_types::Binary,_>(b"Tree".as_slice())
        .bind::<xmtp_db::diesel::sql_types::Binary,_>(b"ConfirmationTag".as_slice())
        .bind::<xmtp_db::diesel::sql_types::Binary,_>(b"InterimTranscriptHash".as_slice())
        .bind::<xmtp_db::diesel::sql_types::Binary,_>(b"ProposalRefs".as_slice()).execute(c))?;
    assert!(MlsGroup::load(provider.storage(), &gid)?.is_none());
    let public_after = PublicGroup::load(provider.storage(), &gid)?
        .expect("public state survives secrets removal");
    let after_members: Vec<String> = public_after
        .members()
        .map(|m| MlsCredential::decode(m.credential.serialized_content()).map(|c| c.inbox_id))
        .collect::<Result<_, _>>()?;
    assert_eq!(after_members, members);
    let context_after: GroupContext = provider
        .storage()
        .group_context(&gid)?
        .expect("context survives");
    assert_eq!(
        extract_group_metadata(context_after.extensions())?,
        immutable
    );
    assert_eq!(
        extract_legacy_group_mutable_metadata_from_extensions(context_after.extensions())?,
        mutable
    );
    conn.raw_query(|c| {
        xmtp_db::diesel::sql_query(
            "DELETE FROM openmls_key_value WHERE substr(key_bytes, 1, 12) != ?",
        )
        .bind::<xmtp_db::diesel::sql_types::Binary, _>(b"GroupContext".as_slice())
        .execute(c)
    })?;
    assert!(PublicGroup::load(provider.storage(), &gid)?.is_none());
    let context_only: GroupContext = provider
        .storage()
        .group_context(&gid)?
        .expect("context without tree");
    assert_eq!(
        extract_group_metadata(context_only.extensions())?,
        immutable
    );
    assert_eq!(
        extract_legacy_group_mutable_metadata_from_extensions(context_only.extensions())?,
        mutable
    );
    let report = serde_json::json!({ "legacy_commit":"cc878025", "group_id":hex::encode(gid.as_slice()), "key_hex":hex::encode(&key), "storage_version":CURRENT_VERSION, "context_bytes":bytes.len(), "creator_inbox_id":decoded_immutable.creator_inbox_id, "conversation_type":format!("{:?}", decoded_immutable.conversation_type), "dm_members":decoded_immutable.dm_members, "attributes":decoded_mutable.attributes, "admin_list":decoded_mutable.admin_list, "super_admin_list":decoded_mutable.super_admin_list, "members":members, "full_mls_load_without_secrets":false, "public_group_load_without_secrets":true, "app_data_attributes":app_mutable.attributes, "malformed_optional_components":errors.len() });
    fs::write(
        format!("{out}/normalized.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
