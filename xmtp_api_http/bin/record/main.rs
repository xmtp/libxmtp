use color_eyre::{Result, eyre::eyre};
use const_format::concatcp;
use futures::StreamExt;
use httpmock::{MockServer, Recording};
use prost::Message;
use reqwest::header::HeaderMap;
use xmtp_configuration::RestApiEndpoints;
use xmtp_proto::xmtp::mls::api::v1::SubscribeGroupMessagesRequest;
use xmtp_proto::xmtp::mls::api::v1::subscribe_group_messages_request::Filter;

mod subscribe_group_messages;
use subscribe_group_messages::*;

///! Record and HTTP Request sent to local
#[tokio::main]
pub async fn main() -> Result<()> {
    clear_recordings()?;
    let server = MockServer::start();
    let recording = begin_recording(&server)?;
    clear()?;
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse()?);
    headers.insert("Accept", "application/x-protobuf".parse()?);
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();
    let mut path = String::new();
    path.push_str(&server.base_url());
    path.push_str(RestApiEndpoints::SUBSCRIBE_GROUP_MESSAGES);

    let group_id = generate_group()?;
    let request = SubscribeGroupMessagesRequest {
        filters: vec![Filter {
            group_id: group_id.to_vec(),
            id_cursor: 0,
        }],
    };
    let body = request.encode_to_vec();
    let json_body = serde_json::to_string(&request)?;
    println!("pb body {}\njson body {}", &hex::encode(&body), &json_body);

    let request = client
        .post(format!(
            "{}{}",
            server.base_url(),
            RestApiEndpoints::SUBSCRIBE_GROUP_MESSAGES
        ))
        .json(&request)
        .build()?;
    println!("sending to={}", request.url());
    let mut response = client.execute(request).await?;
    // let mut response = response.error_for_status()?;
    let msg_count = 20;
    send_messages(msg_count)?;
    // let mut s = response.bytes_stream();
    loop {
        let mut counter = 0;
        let next = response.chunk().await;
        println!("{:?}", next);
        counter += 1;
        if counter >= 20 || next.unwrap().is_none() {
            break;
        }
    }
    // let msgs = s.as_mut().take(msg_count).collect::<Vec<_>>().await;
    // println!("Messages: {:?}", msgs);
    // clear()?;
    save_recording(&recording)?;

    Ok(())
}
