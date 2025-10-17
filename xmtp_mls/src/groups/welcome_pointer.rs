use super::GroupError;
use xmtp_common::{Retry, retry_async};
use xmtp_proto::prelude::XmtpMlsClient;
use xmtp_proto::types::{DecryptedWelcomePointer, WelcomeMessageType, WelcomeMessageV1};

/// Returns none if the welcome pointer is not found
pub async fn resolve_welcome_pointer<Context: crate::context::XmtpSharedContext>(
    decrypted_welcome_pointer: &DecryptedWelcomePointer,
    context: &Context,
) -> Result<Option<WelcomeMessageV1>, GroupError> {
    let retry = Retry::default();
    let mut retries = 0;
    let time_spent = xmtp_common::time::Instant::now();

    let decrypted_v1 = decrypted_welcome_pointer;

    tracing::info!(
        "Resolving welcome pointer for destination {}",
        decrypted_v1.destination
    );

    // Can't use retry_async! because we want to return Ok(None) if it isn't resolved.
    let welcome = loop {
        let welcome = retry_async!(
            Retry::default(),
            (context
                .api()
                .api_client
                // TODO: limit this to a single message somehow (maybe an earliest_welcome_message fn)
                .query_welcome_messages(decrypted_v1.destination.as_slice().try_into()?))
        );
        if let Some(first) = welcome
            .map_err(|e| xmtp_api::ApiError::Api(Box::new(e)))?
            .into_iter()
            .next()
        {
            break first;
        }
        retries += 1;
        if retries <= retry.retries()
            && let Some(d) = retry.backoff(retries, time_spent)
        {
            tracing::info!(
                "Welcome pointee not found, backing off for {d:?}... (attempt {})",
                retries
            );
            xmtp_common::time::sleep(d).await;
        } else {
            return Ok(None);
        }
    };
    // These failure modes are non-retryable and will end up incrementing
    // the cursor and will prevent the welcome message from being retried.
    match welcome.variant {
        WelcomeMessageType::V1(v1) => Ok(Some(v1)),
        WelcomeMessageType::WelcomePointer(_) => {
            tracing::warn!("Got Another welcome pointer from a welcome pointer. Ignoring.");
            Err(xmtp_proto::ConversionError::InvalidValue {
                item: "WelcomeMessage.version",
                expected: "V1",
                got: "WelcomePointer".into(),
            }
            .into())
        }
        WelcomeMessageType::DecryptedWelcomePointer(_) => {
            // TODO: this should be unreachable, but leaving it as is for now.
            tracing::warn!("Got a decrypted welcome pointer from a welcome pointer. Ignoring.");
            Err(xmtp_proto::ConversionError::InvalidValue {
                item: "WelcomeMessage.version",
                expected: "V1",
                got: "DecryptedWelcomePointer".into(),
            }
            .into())
        }
    }
}
