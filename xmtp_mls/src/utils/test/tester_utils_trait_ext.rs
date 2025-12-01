use crate::{
    Client,
    context::XmtpSharedContext,
    groups::{GroupError, MlsGroup, intents::UpdateGroupMembershipResult},
};

#[allow(async_fn_in_trait)]
pub trait MlsGroupExt: Sized {
    async fn invite<C2: XmtpSharedContext>(
        &self,
        other: &Client<C2>,
    ) -> Result<UpdateGroupMembershipResult, GroupError>;

    async fn send_msg(&self, m: &[u8]);
}

impl<C: XmtpSharedContext> MlsGroupExt for MlsGroup<C> {
    async fn invite<C2: XmtpSharedContext>(
        &self,
        other: &Client<C2>,
    ) -> Result<UpdateGroupMembershipResult, GroupError> {
        self.add_members_by_inbox_id(&[other.inbox_id()]).await
    }

    async fn send_msg(&self, m: &[u8]) {
        let _ = self.send_message(m, Default::default()).await.unwrap();
    }
}
