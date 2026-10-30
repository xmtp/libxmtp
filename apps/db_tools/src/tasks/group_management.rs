use anyhow::Result;
use xmtp_db::{
    ConnectionExt, DbConnection, Fetch,
    consent_record::{
        ConsentKey, ConsentState, ConsentType, ConsentWrite, Decision, Owed, write_consent,
    },
    identity::StoredIdentity,
};

/// Set each group's consent as the user would, as a decision. A DM's
/// consent is its peer's inbox record.
fn set_groups<C: ConnectionExt>(
    conn: &DbConnection<C>,
    group_ids: &[&[u8]],
    state: ConsentState,
) -> Result<()> {
    let identity: Option<StoredIdentity> = conn.fetch(&())?;
    let own_inbox = identity
        .map(|identity| identity.inbox_id)
        .unwrap_or_default();
    for group_id in group_ids {
        let key = ConsentKey::for_app(
            conn,
            &own_inbox,
            ConsentType::ConversationId,
            &hex::encode(group_id),
        )?;
        let decide = ConsentWrite::Decide(Decision::Set(state));
        write_consent(conn, &key, decide, Owed::Mark)?;
    }
    Ok(())
}

pub fn enable_groups<C>(conn: &DbConnection<C>, group_ids: &[&[u8]]) -> Result<()>
where
    C: ConnectionExt,
{
    set_groups(conn, group_ids, ConsentState::Allowed)
}

pub fn disable_groups<C>(conn: &DbConnection<C>, group_ids: &[&[u8]]) -> Result<()>
where
    C: ConnectionExt,
{
    set_groups(conn, group_ids, ConsentState::Denied)
}

#[cfg(test)]
mod tests {
    use xmtp_db::consent_record::ConsentState;
    use xmtp_mls::tester;

    use crate::tasks::{disable_groups, enable_groups};

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_disable_groups() {
        tester!(alix);

        let g = alix.create_group(None, None)?;
        disable_groups(&alix.db(), &[g.group_id.as_slice()])?;

        let g = alix.group(&g.group_id)?;
        assert_eq!(g.consent_state()?, ConsentState::Denied);

        enable_groups(&alix.db(), &[g.group_id.as_slice()])?;

        let g = alix.group(&g.group_id)?;
        assert_eq!(g.consent_state()?, ConsentState::Allowed);
    }

    /// A DM's consent lands on its peer's inbox record, which the DM reads.
    // verifies: CONS-052
    #[xmtp_common::test(unwrap_try = true)]
    async fn disabling_a_dm_blocks_its_peer() {
        tester!(alix);
        tester!(bo);

        let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
        disable_groups(&alix.db(), &[dm.group_id.as_slice()])?;

        assert_eq!(
            alix.group(&dm.group_id)?.consent_state()?,
            ConsentState::Denied
        );
    }
}
