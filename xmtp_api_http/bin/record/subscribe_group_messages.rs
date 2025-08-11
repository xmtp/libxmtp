//! Records a subscribe request to xmtp-node-go
use color_eyre::{Result, eyre::eyre};
use const_format::concatcp;
use httpmock::{MockServer, Recording};
use prost::Message;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use xmtp_common::types::GroupId;
use xmtp_configuration::RestApiEndpoints;
use xmtp_proto::xmtp::mls::api::v1::SubscribeGroupMessagesRequest;
use xmtp_proto::xmtp::mls::api::v1::subscribe_group_messages_request::Filter;
use xshell::{Shell, cmd};

pub const RECORDINGS_PATH: &str = concatcp!(env!("CARGO_MANIFEST_DIR"), "/", "recordings");
const XDBG: &str = concatcp!(env!("CARGO_WORKSPACE_DIR"), "target/release/xdbg");

#[derive(Serialize, Deserialize)]
pub struct Group {
    id: String,
    group: serde_json::Value,
}

pub(super) fn generate_group() -> Result<GroupId> {
    let sh = Shell::new()?;
    cmd!(
        sh,
        "{XDBG} -q -b local generate --entity identity --amount 2"
    )
    .run()?;
    cmd!(
        sh,
        "{XDBG} -q -b local generate --entity group --amount 1 --invite 2"
    )
    .run()?;
    let group_json = cmd!(sh, "{XDBG} -q -b local export -e group").read()?;
    let group_json: Vec<Group> = serde_json::from_str(&group_json)?;
    let group_id = group_json[0].id.parse()?;
    println!("group_id={}", group_id);
    Ok(group_id)
}

pub(super) fn clear_recordings() -> Result<()> {
    let path = PathBuf::from_str(RECORDINGS_PATH)?;
    let _ = std::fs::remove_dir_all(path);
    Ok(())
}

/// Clears the local database of identities and groups
pub(super) fn clear() -> Result<()> {
    let sh = Shell::new()?;
    cmd!(sh, "{XDBG} -q -b local --clear").run()?;
    Ok(())
}

pub(super) fn send_messages(n: usize) -> Result<()> {
    let sh = Shell::new()?;
    let n = n.to_string();
    cmd!(
        sh,
        "{XDBG} -q -b local generate --entity message --amount {n}"
    )
    .run()?;
    Ok(())
}

/// begin a recording on "server'
pub(super) fn begin_recording<'a>(server: &'a MockServer) -> Result<Recording<'a>> {
    server.forward_to(xmtp_configuration::HttpGatewayUrls::NODE, |rule| {
        rule.filter(|when| {
            when.path(RestApiEndpoints::SUBSCRIBE_GROUP_MESSAGES);
        });
    });

    let recording = server.record(|rule| {
        rule.record_response_delays(true)
            .record_request_headers(vec!["Accept", "Content-Type"])
            .filter(|when| {
                when.path(RestApiEndpoints::SUBSCRIBE_GROUP_MESSAGES);
            });
    });
    Ok(recording)
}

pub(super) fn save_recording(recording: &Recording<'_>) -> Result<()> {
    let path = PathBuf::from_str(RECORDINGS_PATH)?;
    recording
        .save_to(path, "xmtp_node_go_subscribe_group_messages")
        .map_err(|e| eyre!("failed to save recording {e}"))?;
    Ok(())
}
