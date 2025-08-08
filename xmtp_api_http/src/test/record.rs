use reqwest::header::HeaderMap;
use std::process::Command;
use xmtp_common::types::GroupId;

///! Record and HTTP Request sent to local
pub async fn record_subscribe_welcomes(url: &str) -> anyhow::Result<()> {
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "application/x-protobuf".parse()?);
    headers.insert("Accept", "application/x-protobuf".parse()?);

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();
    let mut path = String::new();
    path.push_str(url);
    path.push_str(xmtp_configuration::RestApiEndpoints::SUBSCRIBE_GROUP_MESSAGES);
    let response = client.post(url).send().await;

    Ok(())
}

pub fn generate_group() -> GroupId {
    let cmd = Command::new("cargo").args([
        "xdbg", "-b", "local", "generate", "--entity", "identity", "--amount", "2",
    ]);
    let cmd = Command::new("cargo").args([
        "xdbg", "-b", "local", "generate", "--entity", "group", "--amount", "1",
    ]);

    Default::default()
}

pub fn clear() {
    let cmd = Command::new("cargo").args(["xdbg", "--clear"]);
}
