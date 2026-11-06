//! Profile field tests: profile values.

use super::*;

const MAX_ELEMENT: usize = 8192;
/// A member sets its own display name, but neither the sender nor a
/// receiver lets it write another member's.
// verifies: PERM-027
#[xmtp_common::test(unwrap_try = true)]
async fn test_display_name_is_self_owned() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

    write(
        &group,
        ComponentId::USER_DISPLAY_NAME,
        insert_name(alix.inbox_id(), b"Alix"),
    )
    .await?;
    bo_group.sync().await?;
    let expected = BTreeMap::from([(inbox(alix.inbox_id()), b"Alix".to_vec())]);
    assert_eq!(display_names(&bo_group)?, expected);

    let forged = insert_name(bo.inbox_id(), b"Not Bo");
    assert!(
        write(&group, ComponentId::USER_DISPLAY_NAME, forged.clone())
            .await
            .is_err()
    );
    assert_peer_rejects(
        &group,
        &bo_group,
        vec![(ComponentId::USER_DISPLAY_NAME, forged)],
    )
    .await?;
    assert_eq!(display_names(&bo_group)?, expected);
}

/// Receivers reject a display name that is not UTF-8.
// verifies: META-010
#[xmtp_common::test(unwrap_try = true)]
async fn test_display_name_must_be_utf8() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

    let invalid = insert_name(alix.inbox_id(), &[0xC3, 0x28]);
    assert_peer_rejects(
        &group,
        &bo_group,
        vec![(ComponentId::USER_DISPLAY_NAME, invalid)],
    )
    .await?;
    assert!(display_names(&bo_group)?.is_empty());
}

/// A display name or group image may hold 8192 bytes, and receivers reject
/// one byte more.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
async fn test_profile_values_are_bounded() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

    let name = vec![b'n'; MAX_ELEMENT];
    write(
        &group,
        ComponentId::USER_DISPLAY_NAME,
        insert_name(alix.inbox_id(), &name),
    )
    .await?;
    write(&group, ComponentId::GROUP_IMAGE, vec![0xFF; MAX_ELEMENT]).await?;
    for (id, payload) in [
        (
            ComponentId::USER_DISPLAY_NAME,
            TlsMapDelta::<InboxId, VLBytes>::new()
                .update(
                    inbox(alix.inbox_id()),
                    VLBytes::new(vec![b'n'; MAX_ELEMENT + 1]),
                )
                .tls_serialize_detached()?,
        ),
        (ComponentId::GROUP_IMAGE, vec![0xFF; MAX_ELEMENT + 1]),
    ] {
        assert_peer_rejects(&group, &bo_group, vec![(id, payload)]).await?;
    }
    assert_eq!(display_names(&bo_group)?[&inbox(alix.inbox_id())], name);
    assert_eq!(
        value(&bo_group, ComponentId::GROUP_IMAGE)?,
        Some(vec![0xFF; MAX_ELEMENT])
    );
}

/// `GROUP_IMAGE` carries opaque bytes: a value that is neither UTF-8 nor a
/// URL arrives unchanged.
// verifies: META-010
#[xmtp_common::test(unwrap_try = true)]
async fn test_group_image_is_opaque() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

    let image = vec![0x00, 0xFF, 0xC3, 0x28, 0x0A];
    write(&group, ComponentId::GROUP_IMAGE, image.clone()).await?;
    bo_group.sync().await?;
    assert_eq!(value(&bo_group, ComponentId::GROUP_IMAGE)?, Some(image));
}

/// An absent application immutable scalar takes one first value. Every later
/// write is refused, including a second Update in the same commit.
// verifies: META-004
#[xmtp_common::test(unwrap_try = true)]
async fn test_immutable_field_is_written_once() {
    const ONCE: ComponentId = ComponentId::new(0xFD01);
    const PAIR: ComponentId = ComponentId::new(0xFD02);
    let alix = EngineTestPeer::new_with(|o| {
        o.configure(|c| {
            c.application_components = [ONCE, PAIR]
                .map(|id| {
                    definition(
                        id.as_u16(),
                        ComponentType::String,
                        MetadataBasePolicy::Allow,
                        true,
                        false,
                    )
                })
                .into()
        })
    })
    .await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

    write(&group, ONCE, b"first".to_vec()).await?;
    assert!(write(&group, ONCE, b"second".to_vec()).await.is_err());
    assert_peer_rejects(&group, &bo_group, vec![(ONCE, b"second".to_vec())]).await?;
    assert_eq!(value(&bo_group, ONCE)?, Some(b"first".to_vec()));

    let pair = vec![(PAIR, b"a".to_vec()), (PAIR, b"b".to_vec())];
    assert_peer_rejects(&group, &bo_group, pair).await?;
    assert_eq!(value(&bo_group, PAIR)?, None);
}
