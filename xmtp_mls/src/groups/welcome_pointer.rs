use super::GroupError;
use xmtp_common::{Retry, retry_async};
use xmtp_proto::mls_v1::welcome_message::{V1, Version};
use xmtp_proto::prelude::XmtpMlsClient;

pub async fn resolve_welcome_pointer<Context: crate::context::XmtpSharedContext>(
    decrypted_welcome_pointer: &xmtp_proto::xmtp::mls::message_contents::WelcomePointer,
    context: &Context,
) -> Result<V1, GroupError> {
    let retry = Retry::default();
    let mut retries = 0;
    let time_spent = xmtp_common::time::Instant::now();

    let decrypted_v1 = match &decrypted_welcome_pointer.version {
        Some(xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::V1(v1)) => v1,
        None => {
            return Err(xmtp_proto::ConversionError::InvalidValue {
                item: "WelcomePointer",
                expected: "WelcomePointer from Node",
                got: "None".into(),
            }
            .into());
        }
    };

    tracing::info!(
        "Resolving welcome pointer for destination {}",
        hex::encode(&decrypted_v1.destination)
    );

    let welcome = loop {
        let welcome = retry_async!(
            Retry::default(),
            (context.api().api_client.query_welcome_messages(
                xmtp_proto::mls_v1::QueryWelcomeMessagesRequest {
                    installation_key: decrypted_v1.destination.clone(),
                    paging_info: Some(xmtp_proto::mls_v1::PagingInfo {
                        id_cursor: 0,
                        limit: 1,
                        direction: xmtp_proto::mls_v1::SortDirection::Ascending as i32,
                    }),
                }
            ))
        );
        if let Some(first) = welcome
            .map_err(|e| xmtp_api::ApiError::Api(Box::new(e)))?
            .messages
            .into_iter()
            .next()
        {
            break first;
        }
        retries += 1;
        tracing::info!(
            "Welcome pointer not found, backing off... (attempt {})",
            retries
        );
        if let Some(d) = retry.backoff(retries, time_spent) {
            xmtp_common::time::sleep(d).await;
        } else {
            return Err(xmtp_proto::ConversionError::InvalidValue {
                item: "WelcomeMessage",
                expected: "WelcomeMessage from Node",
                got: "None".into(),
            }
            .into());
        }
        tracing::debug!("welcome pointer not found, retrying...");
    };
    let welcome = match welcome.version {
        Some(Version::V1(welcome)) => welcome,
        Some(Version::WelcomePointer(_)) => {
            tracing::warn!("Got Another welcome pointer from a welcome pointer. Ignoring.");
            return Err(xmtp_proto::ConversionError::InvalidValue {
                item: "WelcomeMessage.version",
                expected: "V1",
                got: "WelcomePointer".into(),
            }
            .into());
        }
        None => {
            return Err(xmtp_proto::ConversionError::InvalidValue {
                item: "WelcomeMessage.version",
                expected: "V1",
                got: "None".into(),
            }
            .into());
        }
    };
    Ok(welcome)
}
