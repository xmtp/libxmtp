use xmtp_db::consent_record::ConsentState;

use crate::tester;

#[xmtp_common::test(unwrap_try = "true")]
async fn test_auto_consent_to_own_group() {
    tester!(alix1);
    tester!(alix2, from: alix1);

    let g = alix1.create_group(None, None)?;
    g.send_message(b"hello").await?;
    alix2.sync_welcomes().await?;

    let g2 = alix2.group(&g.group_id)?;
    assert_eq!(g2.consent_state()?, ConsentState::Allowed);
}
