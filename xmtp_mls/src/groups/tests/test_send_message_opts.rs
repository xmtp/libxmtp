use crate::groups::send_message_opts::SendMessageOptsBuilder;
use crate::tester;
use xmtp_proto::xmtp::mls::api::v1::group_message::Version;

/// Test that validates the `should_push` field is properly sent to the network
/// when set to true
#[xmtp_common::test(unwrap_try = true)]
async fn test_send_message_should_push() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    // Send a message with should_push set to true
    group
        .send_message(
            b"test message with push",
            SendMessageOptsBuilder::default()
                .should_push(true)
                .build()
                .unwrap(),
        )
        .await?;

    let last_message = group.test_get_last_message_from_network().await?;

    // Extract the V1 message from the Version enum
    let v1_message = match &last_message.version {
        Some(Version::V1(v1)) => v1,
        _ => panic!("Expected V1 message"),
    };

    // Verify should_push is true for the first sent message
    assert!(
        v1_message.should_push,
        "Expected should_push to be true on the network"
    );

    // Send a message with should_push set to false
    group
        .send_message(
            b"test message with push",
            SendMessageOptsBuilder::default()
                .should_push(false)
                .build()
                .unwrap(),
        )
        .await?;

    let last_message = group.test_get_last_message_from_network().await?;

    // Extract the V1 message from the Version enum
    let v1_message = match &last_message.version {
        Some(Version::V1(v1)) => v1,
        _ => panic!("Expected V1 message"),
    };

    // Verify should_push is false for the second message
    assert!(
        !v1_message.should_push,
        "Expected should_push to be false on the network"
    );
}
