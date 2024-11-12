use chrono::{DateTime, Utc};
use clap::Subcommand;
use openmls::prelude::{tls_codec::Deserialize, MlsMessageBodyIn, MlsMessageIn};
use xmtp_mls::groups::scoped_client::ScopedGroupClient;
use xmtp_mls::{Client, XmtpApi};
use xmtp_proto::xmtp::mls::api::v1::group_message::Version as GroupMessageVersion;

#[derive(Debug, Subcommand)]
pub enum DebugCommands {
    GroupMessages {
        #[arg(short, long)]
        group_id: String,
    },
}

fn format_timestamp(timestamp_ns: u64) -> String {
    let datetime: DateTime<Utc> = DateTime::from_timestamp_nanos(timestamp_ns as i64);
    datetime.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string()
}

pub async fn debug_group_messages(
    client: &Client<Box<dyn XmtpApi>>,
    group_id: Vec<u8>,
) -> Result<(), String> {
    let api_client = client.api();
    let envelopes = api_client
        .query_group_messages(group_id, None)
        .await
        .unwrap();
    for envelope in envelopes {
        let msgv1 = match &envelope.version {
            Some(GroupMessageVersion::V1(value)) => value,
            _ => return Err("Invalid group message version".to_string()),
        };
        let body = match MlsMessageIn::tls_deserialize_exact(&msgv1.data)
            .map_err(|e| e.to_string())?
            .extract()
        {
            MlsMessageBodyIn::PrivateMessage(message) => message,
            _ => return Err("Unsupported message type".to_string()),
        };
        let timestamp = format_timestamp(msgv1.created_ns);
        let sequence_id = msgv1.id;
        let epoch = body.epoch().as_u64();
        let content_type = body.content_type();
        info!("[{timestamp}] [Epoch {epoch}] [Seq {sequence_id}] {content_type:?}");
    }

    Ok(())
}

pub async fn handle_debug(
    client: &Client<Box<dyn XmtpApi>>,
    command: &DebugCommands,
) -> Result<(), String> {
    match command {
        DebugCommands::GroupMessages { group_id } => {
            info!("Querying group messages for group id: {}", group_id);
            debug_group_messages(client, hex::decode(group_id).expect("group id decode")).await
        }
    }
}
