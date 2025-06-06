use xmtp_db::consent_record::ConsentState;

use crate::tester;

#[xmtp_common::test(unwrap_try = "true")]
async fn test_auto_consent_to_own_group() {
    tester!(alix1);

    tester!(bo);
    let unwanted_bo = bo
        .create_group_with_inbox_ids(&[alix1.inbox_id()], None, None)
        .await?;

    alix1.sync_welcomes().await?;
    let unwanted = alix1.group(&unwanted_bo.group_id)?;
    // We were added by ourselves, but we did not consent to the group. The group should remain unconsented.
    assert_eq!(unwanted.consent_state()?, ConsentState::Unknown);

    tester!(alix2, from: alix1);
    unwanted_bo.send_message(b"hi unwanted group").await?;

    alix2.sync_welcomes().await?;
    let unwanted2 = alix2.group(&unwanted.group_id)?;
    assert_eq!(unwanted2.consent_state()?, ConsentState::Unknown);

    let g = alix1.create_group(None, None)?;
    g.send_message(b"hello").await?;
    alix2.sync_welcomes().await?;

    let g2 = alix2.group(&g.group_id)?;
    assert_eq!(g2.consent_state()?, ConsentState::Allowed);
}
