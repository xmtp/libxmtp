use anyhow::Result;
use reqwest::header::HeaderMap;
use xmtp_common::types::GroupId;
use xshell::{Shell, cmd};

///! Record and HTTP Request sent to local
pub async fn record_subscribe_welcomes(url: &str) -> Result<()> {
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

pub fn generate_group() -> Result<GroupId> {
    let sh = Shell::new()?;
    let xdbg = "cargo xdbg -b local";
    cmd!(sh, "{xdbg} --clear").run()?;
    cmd!(sh, "{xdbg} generate --entity identity --amount 2").run()?;
    cmd!(sh, "{xdbg} generate --entity group --amount 1 --invite 2").run()?;
    let group_json = cmd!(sh, "{xdbg} export -e group").read()?;
    cmd!(sh, "{xdbg} --clear").run()?;

    Ok(Default::default())
}
