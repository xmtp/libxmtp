use openmls::group::MlsGroup as OpenMlsGroup;
use xmtp_db::{NotFound, StorageError, XmtpMlsStorageProvider};
use xmtp_proto::types::GroupId;

use crate::groups::mls_sync::GroupMessageProcessingError;

pub trait MlsGroupReload {
    fn reload<S: XmtpMlsStorageProvider>(
        &mut self,
        provider: &S,
    ) -> impl std::future::Future<Output = Result<(), GroupMessageProcessingError>>
    + xmtp_common::MaybeSend;
}

impl MlsGroupReload for OpenMlsGroup {
    async fn reload<S: XmtpMlsStorageProvider>(
        &mut self,
        provider: &S,
    ) -> Result<(), GroupMessageProcessingError> {
        *self = (OpenMlsGroup::load(provider, self.group_id()))
            .await?
            .ok_or(StorageError::NotFound(NotFound::MlsGroup(
                GroupId::try_from(self.group_id())?,
            )))?;
        Ok(())
    }
}
