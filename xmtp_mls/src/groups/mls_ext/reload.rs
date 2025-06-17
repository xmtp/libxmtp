use openmls::group::MlsGroup as OpenMlsGroup;
use xmtp_db::{ConnectionExt, MlsProviderExt, NotFound, StorageError, XmtpOpenMlsProvider};

use crate::groups::mls_sync::GroupMessageProcessingError;

pub trait MlsGroupReload {
    fn reload<C: ConnectionExt>(
        &mut self,
        provider: &XmtpOpenMlsProvider<C>,
    ) -> Result<(), GroupMessageProcessingError>;
}

impl MlsGroupReload for OpenMlsGroup {
    fn reload<C: ConnectionExt>(
        &mut self,
        provider: &XmtpOpenMlsProvider<C>,
    ) -> Result<(), GroupMessageProcessingError> {
        *self = OpenMlsGroup::load(provider.key_store(), &self.group_id())?
            .ok_or(StorageError::NotFound(NotFound::MlsGroup))?;
        Ok(())
    }
}
