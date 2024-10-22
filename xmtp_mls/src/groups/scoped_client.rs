use std::sync::Arc;

use xmtp_id::{associations::AssociationState, scw_verifier::SmartContractSignatureVerifier};
use xmtp_proto::{api_client::trait_impls::XmtpApi, xmtp::mls::api::v1::GroupMessage};

use crate::{
    api::ApiClientWrapper,
    client::{ClientError, XmtpMlsLocalContext},
    identity_updates::{InstallationDiff, InstallationDiffError},
    intents::Intents,
    storage::{DbConnection, EncryptedMessageStore},
    verified_key_package_v2::VerifiedKeyPackageV2,
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Client,
};

use super::group_membership::{GroupMembership, MembershipDiff};

#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(ScopedGroupClient: Send ))]
#[cfg(not(target_arch = "wasm32"))]
#[allow(unused)]
pub trait LocalScopedGroupClient: Send + Sync + Sized {
    fn api(&self) -> &ApiClientWrapper;

    fn store(&self) -> &EncryptedMessageStore {
        self.context_ref().store()
    }

    fn inbox_id(&self) -> String {
        self.context().inbox_id()
    }

    fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, ClientError> {
        self.context_ref().mls_provider()
    }

    fn intents(&self) -> &Arc<Intents>;

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext>;

    fn context(&self) -> Arc<XmtpMlsLocalContext> {
        self.context_ref().clone()
    }

    async fn get_installation_diff(
        &self,
        conn: &DbConnection,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError>;

    async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<VerifiedKeyPackageV2>, ClientError>;

    async fn get_association_state(
        &self,
        conn: &DbConnection,
        inbox_id: String,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError>;

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection,
        identifiers: &[(String, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError>;

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError>;
}

#[cfg(target_arch = "wasm32")]
#[allow(async_fn_in_trait)]
pub trait ScopedGroupClient: Sized {
    type ApiClient: XmtpApi;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient>;

    fn store(&self) -> &EncryptedMessageStore {
        self.context_ref().store()
    }

    fn inbox_id(&self) -> String {
        self.context().inbox_id()
    }

    fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, ClientError> {
        self.context_ref().mls_provider()
    }

    fn intents(&self) -> &Arc<Intents>;

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext>;

    fn context(&self) -> Arc<XmtpMlsLocalContext> {
        self.context_ref().clone()
    }

    async fn get_installation_diff(
        &self,
        conn: &DbConnection,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError>;

    async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<VerifiedKeyPackageV2>, ClientError>;

    async fn get_association_state(
        &self,
        conn: &DbConnection,
        inbox_id: String,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError>;

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection,
        identifiers: &[(String, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError>;

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError>;
}

impl<Verifier> ScopedGroupClient for Client<Verifier>
where
    Verifier: SmartContractSignatureVerifier + Clone,
{
    fn api(&self) -> &ApiClientWrapper {
        &self.api_client
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext> {
        self.context()
    }

    fn intents(&self) -> &Arc<Intents> {
        crate::Client::<Verifier>::intents(self)
    }

    async fn get_installation_diff(
        &self,
        conn: &DbConnection,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError> {
        crate::Client::<Verifier>::get_installation_diff(
            self,
            conn,
            old_group_membership,
            new_group_membership,
            membership_diff,
        )
        .await
    }

    async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<VerifiedKeyPackageV2>, ClientError> {
        crate::Client::<Verifier>::get_key_packages_for_installation_ids(self, installation_ids)
            .await
    }

    async fn get_association_state(
        &self,
        conn: &DbConnection,
        inbox_id: String,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        crate::Client::<Verifier>::get_association_state(self, conn, inbox_id, to_sequence_id).await
    }

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection,
        identifiers: &[(String, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        crate::Client::<Verifier>::batch_get_association_state(self, conn, identifiers).await
    }

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        crate::Client::<Verifier>::query_group_messages(self, group_id, conn).await
    }
}
