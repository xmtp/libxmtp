use super::*;
use crate::Client;
use crate::XmtpApi;
use xmtp_configuration::DeviceSyncUrls;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group::StoredGroup;
use xmtp_db::group_message::MsgQueryArgs;
impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
{
    pub(super) fn syncable_groups(&self) -> Result<Vec<Syncable>, DeviceSyncError> {
        let provider = self.mls_provider();
        let groups = provider
            .db()
            .find_groups(GroupQueryArgs::default())?
            .into_iter()
            .map(Syncable::Group)
            .collect();

        Ok(groups)
    }

    pub(super) fn syncable_messages(&self) -> Result<Vec<Syncable>, DeviceSyncError> {
        let groups = self.context.db().find_groups(GroupQueryArgs::default())?;

        let mut all_messages = vec![];
        for StoredGroup { id, .. } in groups.into_iter() {
            let messages = provider
                .db()
                .get_group_messages(&id, &MsgQueryArgs::default())?;
            for msg in messages {
                all_messages.push(Syncable::GroupMessage(msg));
            }
        }

        Ok(all_messages)
    }
}
