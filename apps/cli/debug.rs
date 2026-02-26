use chrono::{DateTime, Utc};
use clap::Subcommand;
use openmls::prelude::ProtocolMessage;
use openmls::prelude::{MlsMessageBodyIn, MlsMessageIn, OpenMlsProvider, tls_codec::Deserialize};
use std::collections::HashMap;
use xmtp_api::GetIdentityUpdatesV2Filter;
use xmtp_id::InboxUpdate;
use xmtp_id::associations::unverified::UnverifiedAction;
use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::verified_key_package_v2::VerifiedKeyPackageV2;

#[derive(Debug, Subcommand)]
pub enum DebugCommands {
    GroupMessages {
        #[arg(value_name = "Group ID")]
        group_id: String,
    },
    WelcomeMessages {
        #[arg(value_name = "Installation ID")]
        installation_id: String,
    },
    IdentityUpdates {
        #[arg(value_name = "Inbox ID")]
        inbox_id: String,
    },
    KeyPackages {
        #[arg(value_name = "Installation ID")]
        installation_id: String,
    },
}

fn format_timestamp(timestamp_ns: u64) -> String {
    let datetime: DateTime<Utc> = DateTime::from_timestamp_nanos(timestamp_ns as i64);
    datetime.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string()
}

pub async fn debug_group_messages(client: &crate::Client, group_id: Vec<u8>) -> Result<(), String> {
    let api_client = client.context.api();
    let envelopes = api_client
        .query_group_messages(group_id.into())
        .await
        .unwrap();
    for envelope in envelopes {
        let body = match envelope.message {
            ProtocolMessage::PrivateMessage(message) => message,
            _ => return Err("Unsupported message type".to_string()),
        };
        let timestamp = envelope.created_ns;
        let sequence_id = envelope.cursor;
        let epoch = body.epoch().as_u64();
        let content_type = body.content_type();
        info!("[{timestamp}] [Epoch {epoch}] [Seq {sequence_id}] {content_type:?}");
    }

    Ok(())
}

pub async fn debug_welcome_messages(
    client: &crate::Client,
    installation_id: Vec<u8>,
) -> Result<(), String> {
    let api_client = client.context.api();
    let envelopes = api_client
        .query_welcome_messages(&installation_id)
        .await
        .unwrap();
    for envelope in envelopes {
        let Some(v1) = envelope.as_v1() else {
            tracing::debug!("Welcome pointers not supported");
            continue;
        };
        let body = match MlsMessageIn::tls_deserialize_exact(&v1.data)
            .map_err(|e| e.to_string())?
            .extract()
        {
            MlsMessageBodyIn::PrivateMessage(message) => message,
            _ => return Err("Unsupported message type".to_string()),
        };
        let timestamp = envelope.created_ns;
        let sequence_id = envelope.cursor;
        let epoch = body.epoch().as_u64();
        let content_type = body.content_type();
        info!("[{timestamp}] [Epoch {epoch}] [Seq {sequence_id}] {content_type:?}");
    }

    Ok(())
}

pub async fn debug_key_packages(
    client: &crate::Client,
    installation_id: Vec<u8>,
) -> Result<(), String> {
    let api_client = client.context.api();

    let key_package_results = api_client
        .fetch_key_packages(vec![installation_id])
        .await
        .unwrap();

    let mls_provider = client.context.mls_provider();

    let envelopes: Result<Vec<VerifiedKeyPackageV2>, _> = key_package_results
        .values()
        .map(|bytes| VerifiedKeyPackageV2::from_bytes(mls_provider.crypto(), bytes.as_slice()))
        .collect();

    for envelope in envelopes.unwrap() {
        let inbox_id = envelope.credential.inbox_id;
        let pkey = hex::encode(envelope.installation_public_key);
        info!("[InboxId {inbox_id}]  [Key Packages {pkey}] ");
    }

    Ok(())
}

pub async fn debug_identity_updates(
    client: &crate::Client,
    inbox_id: Vec<u8>,
) -> Result<(), String> {
    let api_client = client.context.api();

    let filters = vec![GetIdentityUpdatesV2Filter {
        sequence_id: None,
        inbox_id: hex::encode(inbox_id),
    }];

    let key_package_results: HashMap<_, Vec<InboxUpdate>> = api_client
        .get_identity_updates_v2(filters)
        .await
        .unwrap()
        .collect();

    for (inbox_id, updates) in key_package_results {
        for update in updates {
            let timestamp = format_timestamp(update.server_timestamp_ns);
            let sequence_id = update.sequence_id;

            let action_names = update
                .update
                .actions
                .iter()
                .map(|action| match action {
                    UnverifiedAction::CreateInbox(_) => "CreateInbox".to_string(),
                    UnverifiedAction::AddAssociation(_) => "AddAssociation".to_string(),
                    UnverifiedAction::RevokeAssociation(_) => "RevokeAssociation".to_string(),
                    UnverifiedAction::ChangeRecoveryAddress(_) => {
                        "ChangeRecoveryAddress".to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");

            info!(
                "[{timestamp}] [Seq {sequence_id}] [InboxId {inbox_id}] [Actions {action_names}]"
            );
        }
    }

    Ok(())
}

pub async fn handle_debug(client: &crate::Client, command: &DebugCommands) -> Result<(), String> {
    match command {
        DebugCommands::GroupMessages { group_id } => {
            info!("Querying group messages for group id: {}", group_id);
            debug_group_messages(client, hex::decode(group_id).expect("group id decode")).await
        }
        DebugCommands::WelcomeMessages { installation_id } => {
            info!(
                "Querying welcome messages for installation id: {}",
                installation_id
            );
            debug_welcome_messages(
                client,
                hex::decode(installation_id).expect("installation id decode"),
            )
            .await
        }
        DebugCommands::IdentityUpdates { inbox_id } => {
            info!("Querying identity updates for inbox id: {}", inbox_id);
            debug_identity_updates(client, hex::decode(inbox_id).expect("inbox id decode")).await
        }
        DebugCommands::KeyPackages { installation_id } => {
            info!(
                "Querying key packages for installation id: {}",
                installation_id
            );
            debug_key_packages(
                client,
                hex::decode(installation_id).expect("installation id decode"),
            )
            .await
        }
    }
}
