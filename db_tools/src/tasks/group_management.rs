use anyhow::Result;
use xmtp_common::time::now_ns;
use xmtp_db::{
    ConnectionExt, DbConnection,
    consent_record::{ConsentState, ConsentType, QueryConsentRecord, StoredConsentRecord},
};

pub fn enable_groups<C>(conn: &DbConnection<C>, group_ids: &[&[u8]]) -> Result<()>
where
    C: ConnectionExt,
{
    for group_id in group_ids {
        conn.insert_newer_consent_record(StoredConsentRecord {
            consented_at_ns: now_ns(),
            entity: hex::encode(group_id),
            entity_type: ConsentType::ConversationId,
            state: ConsentState::Allowed,
        })?;
    }

    Ok(())
}

pub fn disable_groups<C>(conn: &DbConnection<C>, group_ids: &[&[u8]]) -> Result<()>
where
    C: ConnectionExt,
{
    for group_id in group_ids {
        conn.insert_newer_consent_record(StoredConsentRecord {
            consented_at_ns: now_ns(),
            entity: hex::encode(group_id),
            entity_type: ConsentType::ConversationId,
            state: ConsentState::Denied,
        })?;
    }

    Ok(())
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
        disable_groups(&alix.db(), &[&g.group_id])?;

        let g = alix.group(&g.group_id)?;
        assert_eq!(g.consent_state()?, ConsentState::Denied);

        enable_groups(&alix.db(), &[&g.group_id])?;

        let g = alix.group(&g.group_id)?;
        assert_eq!(g.consent_state()?, ConsentState::Allowed);
    }
}
