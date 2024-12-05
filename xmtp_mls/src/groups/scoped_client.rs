use super::group_membership::{GroupMembership, MembershipDiff};
use crate::{
    api::ApiClientWrapper,
    client::{ClientError, XmtpMlsLocalContext},
    identity_updates::{InstallationDiff, InstallationDiffError},
    intents::Intents,
    storage::{DbConnection, EncryptedMessageStore, StorageError},
    subscriptions::LocalEvents,
    types::InstallationId,
    verified_key_package_v2::VerifiedKeyPackageV2,
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Client,
};
use std::sync::Arc;
use tokio::sync::broadcast;
use xmtp_id::{
    associations::AssociationState, scw_verifier::SmartContractSignatureVerifier, InboxIdRef,
};
use xmtp_proto::{api_client::trait_impls::XmtpApi, xmtp::mls::api::v1::GroupMessage};

#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(ScopedGroupClient: Send ))]
#[cfg(not(target_arch = "wasm32"))]
#[allow(unused)]
pub trait LocalScopedGroupClient: Send + Sync + Sized {
    type ApiClient: XmtpApi;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient>;

    fn store(&self) -> &EncryptedMessageStore {
        self.context_ref().store()
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents<impl ScopedGroupClient>>;

    fn history_sync_url(&self) -> &Option<String>;

    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.context_ref().inbox_id()
    }

    fn installation_id(&self) -> InstallationId {
        self.context_ref().installation_public_key()
    }

    fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, StorageError> {
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

    async fn get_association_state<'a>(
        &self,
        conn: &DbConnection,
        inbox_id: InboxIdRef<'a>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError>;

    async fn batch_get_association_state<'a>(
        &self,
        conn: &DbConnection,
        identifiers: &[(InboxIdRef<'a>, Option<i64>)],
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

    fn local_events(&self) -> &broadcast::Sender<LocalEvents<impl ScopedGroupClient>>;

    fn history_sync_url(&self) -> &Option<String>;

    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.context_ref().inbox_id()
    }

    fn installation_id(&self) -> &[u8] {
        self.context_ref().installation_public_key()
    }

    fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, StorageError> {
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

    async fn get_association_state<'a>(
        &self,
        conn: &DbConnection,
        inbox_id: InboxIdRef<'a>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError>;

    async fn batch_get_association_state<'a>(
        &self,
        conn: &DbConnection,
        identifiers: &[(InboxIdRef<'a>, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError>;

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError>;
}

impl<ApiClient, Verifier> ScopedGroupClient for Client<ApiClient, Verifier>
where
    ApiClient: XmtpApi,
    Verifier: SmartContractSignatureVerifier,
{
    type ApiClient = ApiClient;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        &self.api_client
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents<impl ScopedGroupClient>> {
        &self.local_events
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext> {
        Client::<ApiClient, Verifier>::context(self)
    }

    fn intents(&self) -> &Arc<Intents> {
        crate::Client::<ApiClient, Verifier>::intents(self)
    }

    fn history_sync_url(&self) -> &Option<String> {
        &self.history_sync_url
    }

    async fn get_installation_diff(
        &self,
        conn: &DbConnection,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError> {
        crate::Client::<ApiClient, Verifier>::get_installation_diff(
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
        crate::Client::<ApiClient, Verifier>::get_key_packages_for_installation_ids(
            self,
            installation_ids,
        )
        .await
    }

    async fn get_association_state<'a>(
        &self,
        conn: &DbConnection,
        inbox_id: InboxIdRef<'a>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        crate::Client::<ApiClient, Verifier>::get_association_state(
            self,
            conn,
            inbox_id,
            to_sequence_id,
        )
        .await
    }

    async fn batch_get_association_state<'a>(
        &self,
        conn: &DbConnection,
        identifiers: &[(InboxIdRef<'a>, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        crate::Client::<ApiClient, Verifier>::batch_get_association_state(self, conn, identifiers)
            .await
    }

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        crate::Client::<ApiClient, Verifier>::query_group_messages(self, group_id, conn).await
    }
}

impl<T> ScopedGroupClient for &T
where
    T: ScopedGroupClient,
{
    type ApiClient = <T as ScopedGroupClient>::ApiClient;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        (**self).api()
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents<impl ScopedGroupClient>> {
        (**self).local_events()
    }

    fn history_sync_url(&self) -> &Option<String> {
        (**self).history_sync_url()
    }

    fn store(&self) -> &EncryptedMessageStore {
        (**self).store()
    }

    fn intents(&self) -> &Arc<Intents> {
        (**self).intents()
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        (**self).inbox_id()
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext> {
        (**self).context_ref()
    }

    fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, StorageError> {
        (**self).mls_provider()
    }

    async fn get_installation_diff(
        &self,
        conn: &DbConnection,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError> {
        (**self)
            .get_installation_diff(
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
        (**self)
            .get_key_packages_for_installation_ids(installation_ids)
            .await
    }

    async fn get_association_state<'a>(
        &self,
        conn: &DbConnection,
        inbox_id: InboxIdRef<'a>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        (**self)
            .get_association_state(conn, inbox_id, to_sequence_id)
            .await
    }

    async fn batch_get_association_state<'a>(
        &self,
        conn: &DbConnection,
        identifiers: &[(InboxIdRef<'a>, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        (**self)
            .batch_get_association_state(conn, identifiers)
            .await
    }

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        (**self).query_group_messages(group_id, conn).await
    }
}

impl<T> ScopedGroupClient for Arc<T>
where
    T: ScopedGroupClient,
{
    type ApiClient = <T as ScopedGroupClient>::ApiClient;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        (**self).api()
    }

    fn store(&self) -> &EncryptedMessageStore {
        (**self).store()
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents<impl ScopedGroupClient>> {
        (**self).local_events()
    }

    fn history_sync_url(&self) -> &Option<String> {
        (**self).history_sync_url()
    }

    fn intents(&self) -> &Arc<Intents> {
        (**self).intents()
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        (**self).inbox_id()
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext> {
        (**self).context_ref()
    }

    fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, StorageError> {
        (**self).mls_provider()
    }

    async fn get_installation_diff(
        &self,
        conn: &DbConnection,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError> {
        (**self)
            .get_installation_diff(
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
        (**self)
            .get_key_packages_for_installation_ids(installation_ids)
            .await
    }

    async fn get_association_state<'a>(
        &self,
        conn: &DbConnection,
        inbox_id: InboxIdRef<'a>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        (**self)
            .get_association_state(conn, inbox_id, to_sequence_id)
            .await
    }

    async fn batch_get_association_state<'a>(
        &self,
        conn: &DbConnection,
        identifiers: &[(InboxIdRef<'a>, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        (**self)
            .batch_get_association_state(conn, identifiers)
            .await
    }

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        (**self).query_group_messages(group_id, conn).await
    }
}
