use openmls::group::MlsGroup as OpenMlsGroup;
use xmtp_db::{
    ConnectionExt, MlsProviderExt, NotFound, StorageError, XmtpMlsStorageProvider,
    XmtpOpenMlsProvider,
};

use crate::groups::mls_sync::GroupMessageProcessingError;

pub trait MlsGroupReload {
    fn reload<S: XmtpMlsStorageProvider>(
        &mut self,
        provider: &S,
    ) -> Result<(), GroupMessageProcessingError>;
}

impl MlsGroupReload for OpenMlsGroup {
    fn reload<S: XmtpMlsStorageProvider>(
        &mut self,
        provider: &S,
    ) -> Result<(), GroupMessageProcessingError> {
        *self = OpenMlsGroup::load(provider, self.group_id())?
            .ok_or(StorageError::NotFound(NotFound::MlsGroup))?;
        Ok(())
    }
}
