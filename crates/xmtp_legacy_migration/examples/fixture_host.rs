//! Browser test helper. This example is not part of any shipped package.
#[cfg(target_arch = "wasm32")]
mod browser {
    use futures::{
        TryStreamExt,
        io::{BufReader, Cursor},
    };
    use wasm_bindgen::prelude::*;
    use xmtp_proto::xmtp::device_sync::backup_element::Element;
    fn error(value: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&value.to_string())
    }

    #[wasm_bindgen]
    pub async fn import_source(path: String, bytes: Vec<u8>) -> Result<(), JsValue> {
        // This isolated fixture worker seeds legacy bytes before any SDK owns
        // the pool. Whole-database SDK restore requires the current schema.
        let cfg = sqlite_wasm_vfs::sahpool::OpfsSAHPoolCfg {
            vfs_name: xmtp_configuration::WASM_VFS_NAME.into(),
            directory: xmtp_configuration::WASM_VFS_DIRECTORY.into(),
            clear_on_init: false,
            initial_capacity: 6,
        };
        let util = sqlite_wasm_vfs::sahpool::install::<sqlite_wasm_rs::WasmOsCallback>(&cfg, true)
            .await
            .map_err(error)?;
        util.import_db(&path, &bytes).map_err(error)?;
        util.pause_vfs().map_err(error)
    }
    #[wasm_bindgen]
    pub async fn export_source(path: String) -> Result<Vec<u8>, JsValue> {
        xmtp_db::export_opfs_database(&path).await.map_err(error)
    }
    #[wasm_bindgen]
    pub async fn inspect_archive_sizes(bytes: Vec<u8>, key: Vec<u8>) -> Result<String, JsValue> {
        let mut importer =
            xmtp_archive::ArchiveImporter::load(Box::pin(BufReader::new(Cursor::new(bytes))), &key)
                .await
                .map_err(error)?;
        let mut values = vec![];
        while let Some(element) = importer.try_next().await.map_err(error)? {
            if let Some(Element::GroupMessage(message)) = element.element {
                values.push(serde_json::json!({
                    "id": hex::encode(message.id),
                    "contentLength": message.decrypted_message_bytes.len(),
                    "contentIsZero": message.decrypted_message_bytes.iter().all(|byte| *byte == 0),
                    "installationLength": message.sender_installation_id.len(),
                    "senderInboxId": message.sender_inbox_id,
                    "authorityId": message.authority_id,
                    "referenceLength": message.reference_id.map(|value| value.len()),
                }));
            }
        }
        serde_json::to_string(&values).map_err(error)
    }

    #[wasm_bindgen]
    pub async fn inspect_archive_record_sizes(
        bytes: Vec<u8>,
        key: Vec<u8>,
    ) -> Result<String, JsValue> {
        let mut importer =
            xmtp_archive::ArchiveImporter::load(Box::pin(BufReader::new(Cursor::new(bytes))), &key)
                .await
                .map_err(error)?;
        let mut values = vec![];
        while let Some(element) = importer.try_next().await.map_err(error)? {
            values.push(match element.element {
                Some(Element::Group(group)) => serde_json::json!({
                    "kind": "group",
                    "id": hex::encode(group.id),
                    "addedByBytes": group.added_by_inbox_id.len(),
                    "addedByIsExpected": group.added_by_inbox_id == "é",
                    "dmBytes": group.dm_id.as_ref().map(|value| value.len()),
                    "dmIsExpected": group.dm_id.as_deref() == Some("d"),
                    "pauseBytes": group.paused_for_version.as_ref().map(|value| value.len()),
                    "pauseIsExpected": group.paused_for_version.as_ref().is_some_and(|value| value.bytes().all(|byte| byte == b'p')),
                }),
                Some(Element::Consent(consent)) => serde_json::json!({
                    "kind": "consent",
                    "entityType": consent.entity_type,
                    "entityBytes": consent.entity.len(),
                    "entityIsExpected": consent.entity.bytes().all(|byte| byte == b'c'),
                    "state": consent.state,
                    "consentedAtNs": consent.consented_at_ns.to_string(),
                }),
                Some(Element::GroupMessage(_)) => serde_json::json!({"kind": "message"}),
                _ => serde_json::json!({"kind": "other"}),
            });
        }
        serde_json::to_string(&values).map_err(error)
    }

    #[wasm_bindgen]
    pub async fn inspect_archive(bytes: Vec<u8>, key: Vec<u8>) -> Result<String, JsValue> {
        let importer =
            xmtp_archive::ArchiveImporter::load(Box::pin(BufReader::new(Cursor::new(bytes))), &key)
                .await
                .map_err(error)?;
        let elements: Vec<_> = importer.try_collect().await.map_err(error)?;
        let values: Vec<_> = elements.into_iter().map(|e| match e.element.unwrap() {
            Element::Group(g) => serde_json::json!({"kind":"group", "id":hex::encode(g.id), "createdAtNs":g.created_at_ns.to_string(), "metadata":g.metadata.map(|m|m.creator_inbox_id), "name":g.mutable_metadata.and_then(|m|m.attributes.get("group_name").cloned())}),
            Element::GroupMessage(m) => serde_json::json!({"kind":"message", "id":hex::encode(m.id), "sentAtNs":m.sent_at_ns.to_string(), "content":hex::encode(m.decrypted_message_bytes)}),
            Element::Consent(c) => serde_json::json!({"kind":"consent", "entity":c.entity, "state":c.state, "consentedAtNs":c.consented_at_ns.to_string()}),
            _ => serde_json::json!({"kind":"other"}),
        }).collect();
        serde_json::to_string(&values).map_err(error)
    }
}
