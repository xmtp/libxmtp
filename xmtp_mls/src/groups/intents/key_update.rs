use crate::groups::mls_ext::MlsGroupExt;
use crate::groups::{mls_ext::GroupIntent, mls_ext::PublishIntentData};
use crate::GroupError;
use openmls::group::MlsGroup;
use openmls::prelude::Extensions;
use openmls::treesync::LeafNodeParameters;
use tls_codec::Serialize;
pub struct KeyUpdateIntent;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl GroupIntent for KeyUpdateIntent {
    async fn publish_data(
        self: Box<Self>,
        provider: &xmtp_db::XmtpOpenMlsProvider,
        context: &crate::client::XmtpMlsLocalContext,
        group: &mut MlsGroup,
        should_push: bool,
    ) -> Result<Option<crate::groups::mls_ext::PublishIntentData>, crate::groups::GroupError> {
        let (commit, _, _) = group.self_update(
            provider,
            &context.identity.installation_keys,
            LeafNodeParameters::default(),
        )?;

        PublishIntentData::builder()
            .payload(commit.tls_serialize_detached()?)
            .staged_commit(group.get_and_clear_pending_commit(provider)?)
            .should_push(should_push)
            .build()
            .map_err(GroupError::from)
            .map(Option::Some)
    }

    fn build_extensions(&self, _group: &MlsGroup) -> Result<Extensions, GroupError> {
        Ok(Extensions::empty())
    }
}
