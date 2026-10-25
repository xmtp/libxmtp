use super::GroupError;
use xmtp_api_backend::envelope::{EnvelopeError, decode_welcome_message};
use xmtp_common::{Retry, retry_async};
use xmtp_proto::{
    types::{DecryptedWelcomePointer, WelcomeMessageType, WelcomeMessageV1},
    xmtp::mls::message_contents::WelcomeWrapperAlgorithm,
};

/// Resolve a welcome pointer from the first envelope on its destination.
/// Returns none if the destination has no envelope.
pub async fn resolve_welcome_pointer<Context: crate::context::XmtpSharedContext>(
    decrypted_welcome_pointer: &DecryptedWelcomePointer,
    context: &Context,
) -> Result<Option<WelcomeMessageV1>, GroupError> {
    let retry = Retry::default();
    let mut retries = 0;
    let time_spent = xmtp_common::time::Instant::now();
    let destination = decrypted_welcome_pointer.destination;
    let max_bytes = usize::try_from(context.incoming_runtime().policy().max_fetched_bytes)
        .unwrap_or(usize::MAX);

    tracing::debug!("Resolving welcome pointer for destination {destination}");

    // Can't use retry_async! because we want to return Ok(None) if it isn't resolved.
    let envelope = loop {
        let envelope = retry_async!(
            Retry::default(),
            (context.api().query_first_welcome(destination, max_bytes))
        );
        if let Some(first) = envelope? {
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
    // No envelope can later appear below the first one, so a first envelope
    // that does not decode is never replaced.
    let welcome = decode_welcome_message(envelope).map_err(|error| match error {
        EnvelopeError::Conversion(error) => GroupError::ConversionError(error),
        error => xmtp_api::ApiError::from(error).into(),
    })?;
    match welcome.variant {
        // implements: JOIN-056
        // The envelope decoder already bound installation_key to the destination topic.
        WelcomeMessageType::V1(v1)
            if v1.wrapper_algorithm == WelcomeWrapperAlgorithm::SymmetricKey =>
        {
            Ok(Some(v1))
        }
        WelcomeMessageType::V1(v1) => Err(xmtp_proto::ConversionError::InvalidValue {
            item: "WelcomePointee.wrapper_algorithm",
            expected: "SymmetricKey",
            got: format!("{:?}", v1.wrapper_algorithm),
        }
        .into()),
        WelcomeMessageType::WelcomePointer(_) => {
            tracing::warn!("Got Another welcome pointer from a welcome pointer. Ignoring.");
            Err(xmtp_proto::ConversionError::InvalidValue {
                item: "WelcomeMessage.version",
                expected: "V1",
                got: "WelcomePointer".into(),
            }
            .into())
        }
    }
}
