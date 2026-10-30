use anyhow::Result;
use xmtp_db::{
    ConnectionExt, DbConnection,
    consent_record::{ConsentKey, ConsentState, ConsentWrite, Decision, Owed, write_consent},
};

/// Set each group's consent as the user would, as a decision.
fn set_groups<C: ConnectionExt>(
    conn: &DbConnection<C>,
    group_ids: &[&[u8]],
    state: ConsentState,
) -> Result<()> {
    for group_id in group_ids {
        let decide = ConsentWrite::Decide(Decision::Set(state));
        write_consent(conn, &ConsentKey::group(group_id), decide, Owed::Mark)?;
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
}
