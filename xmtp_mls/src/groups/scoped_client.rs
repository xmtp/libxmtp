use super::device_sync::handle::{SyncMetric, WorkerHandle};
use super::group_membership::{GroupMembership, MembershipDiff};
use super::{GroupError, MlsGroup};
use crate::utils::VersionInfo;
use crate::verified_key_package_v2::KeyPackageVerificationError;
use crate::{
    client::{ClientError, XmtpMlsLocalContext},
    identity_updates::{InstallationDiff, InstallationDiffError},
    subscriptions::LocalEvents,
    verified_key_package_v2::VerifiedKeyPackageV2,
    Client,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use xmtp_api::ApiClientWrapper;
use xmtp_common::types::InstallationId;
use xmtp_db::XmtpDb;
use xmtp_db::{DbConnection, XmtpOpenMlsProvider};
use xmtp_id::{associations::AssociationState, AsIdRef, InboxIdRef};
use xmtp_proto::{api_client::trait_impls::XmtpApi, xmtp::mls::api::v1::GroupMessage};

#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(ScopedGroupClient: Send))]
#[cfg(not(target_arch = "wasm32"))]
#[allow(unused)]
pub trait LocalScopedGroupClient: Send + Sync + Sized {
    type ApiClient: XmtpApi;
    type Db: XmtpDb;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient>;

    fn store(&self) -> &Self::Db {
        self.context_ref().store()
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents>;

    fn worker_handle(&self) -> Option<Arc<WorkerHandle<SyncMetric>>>;

    fn version_info(&self) -> &Arc<VersionInfo>;

    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.context_ref().inbox_id()
    }

    fn installation_id(&self) -> InstallationId {
        self.context_ref().installation_public_key()
    }

    fn mls_provider(&self) -> XmtpOpenMlsProvider<<Self::Db as XmtpDb>::Connection> {
        self.context_ref().mls_provider()
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext<Self::Db>>;

    fn context(&self) -> Arc<XmtpMlsLocalContext<Self::Db>> {
        self.context_ref().clone()
    }

    /// DB Conncection for higher-level queries
    fn db(&self) -> DbConnection<<Self::Db as XmtpDb>::Connection> {
        self.context().db()
    }

    async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Self>>, GroupError>;

    async fn get_installation_diff(
        &self,
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError>;

    async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        ClientError,
    >;

    async fn get_association_state(
        &self,
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
        inbox_id: InboxIdRef<'_>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError>;

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
        identifiers: &[(impl AsIdRef, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError>;

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
    ) -> Result<Vec<GroupMessage>, ClientError>;
}

#[cfg(target_arch = "wasm32")]
#[allow(async_fn_in_trait)]
pub trait ScopedGroupClient: Sized {
    type ApiClient: XmtpApi;
    type Db: XmtpDb;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient>;

    fn store(&self) -> &Self::Db {
        self.context_ref().store()
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents>;

    fn worker_handle(&self) -> Option<Arc<WorkerHandle<SyncMetric>>>;

    fn version_info(&self) -> &Arc<VersionInfo>;

    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.context_ref().inbox_id()
    }

    fn installation_id(&self) -> InstallationId {
        self.context_ref().installation_public_key()
    }

    fn mls_provider(&self) -> XmtpOpenMlsProvider<<Self::Db as XmtpDb>::Connection> {
        self.context_ref().mls_provider()
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext<Self::Db>>;

    fn context(&self) -> Arc<XmtpMlsLocalContext<Self::Db>> {
        self.context_ref().clone()
    }

    fn db(&self) -> DbConnection<<Self::Db as XmtpDb>::Connection> {
        self.context().db()
    }

    async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Self>>, GroupError>;

    async fn get_installation_diff(
        &self,
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError>;

    async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        ClientError,
    >;

    async fn get_association_state(
        &self,
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
        inbox_id: InboxIdRef<'_>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError>;

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
        identifiers: &[(impl AsIdRef, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError>;

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
    ) -> Result<Vec<GroupMessage>, ClientError>;
}

impl<ApiClient, Db> ScopedGroupClient for Client<ApiClient, Db>
where
    ApiClient: XmtpApi,
    Db: XmtpDb + Send + Sync,
{
    type ApiClient = ApiClient;
    type Db = Db;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        &self.api_client
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        &self.local_events
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext<Self::Db>> {
        Client::<ApiClient, Db>::context(self)
    }

    fn worker_handle(&self) -> Option<Arc<WorkerHandle<SyncMetric>>> {
        self.device_sync.worker_handle()
    }

    fn version_info(&self) -> &Arc<VersionInfo> {
        &self.version_info
    }

    async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Self>>, GroupError> {
        crate::Client::<ApiClient, Db>::sync_welcomes(self).await
    }

    async fn get_installation_diff(
        &self,
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError> {
        crate::Client::<ApiClient, Db>::get_installation_diff(
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
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        ClientError,
    > {
        crate::Client::<ApiClient, Db>::get_key_packages_for_installation_ids(
            self,
            installation_ids,
        )
        .await
    }

    async fn get_association_state(
        &self,
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
        inbox_id: InboxIdRef<'_>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        crate::Client::<ApiClient, Db>::get_association_state(self, conn, inbox_id, to_sequence_id)
            .await
    }

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
        identifiers: &[(impl AsIdRef, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        crate::Client::<ApiClient, Db>::batch_get_association_state(self, conn, identifiers).await
    }

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection<<Self::Db as XmtpDb>::Connection>,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        crate::Client::<ApiClient, Db>::query_group_messages(self, group_id, conn).await
    }
}

impl<T> ScopedGroupClient for &T
where
    T: ScopedGroupClient,
{
    type ApiClient = <T as ScopedGroupClient>::ApiClient;
    type Db = <T as ScopedGroupClient>::Db;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        (**self).api()
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        (**self).local_events()
    }

    fn worker_handle(&self) -> Option<Arc<WorkerHandle<SyncMetric>>> {
        (**self).worker_handle()
    }

    fn version_info(&self) -> &Arc<VersionInfo> {
        (**self).version_info()
    }

    fn store(&self) -> &Self::Db {
        (**self).store()
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        (**self).inbox_id()
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext<Self::Db>> {
        (**self).context_ref()
    }

    fn mls_provider(&self) -> XmtpOpenMlsProvider<<Self::Db as XmtpDb>::Connection> {
        (**self).mls_provider()
    }

    async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Self>>, GroupError> {
        // Get inner groups
        let inner_result = (**self).sync_welcomes().await?;

        // Create new vector with the correct type
        let mut result = Vec::with_capacity(inner_result.len());

        // For each group in the result
        for group in inner_result {
            // Create a new MlsGroup with reference to self
            let new_group = MlsGroup::new(
                *self,
                group.group_id.clone(),
                group.dm_id.clone(),
                group.created_at_ns,
            );
            result.push(new_group);
        }

        Ok(result)
    }

    async fn get_installation_diff(
        &self,
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
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
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        ClientError,
    > {
        (**self)
            .get_key_packages_for_installation_ids(installation_ids)
            .await
    }

    async fn get_association_state(
        &self,
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
        inbox_id: InboxIdRef<'_>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        (**self)
            .get_association_state(conn, inbox_id, to_sequence_id)
            .await
    }

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
        identifiers: &[(impl AsIdRef, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        (**self)
            .batch_get_association_state(conn, identifiers)
            .await
    }

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        (**self).query_group_messages(group_id, conn).await
    }
}

impl<T> ScopedGroupClient for Arc<T>
where
    T: ScopedGroupClient,
{
    type ApiClient = <T as ScopedGroupClient>::ApiClient;
    type Db = <T as ScopedGroupClient>::Db;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        (**self).api()
    }

    fn store(&self) -> &<T as ScopedGroupClient>::Db {
        (**self).store()
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        (**self).local_events()
    }

    fn worker_handle(&self) -> Option<Arc<WorkerHandle<SyncMetric>>> {
        (**self).worker_handle()
    }

    fn version_info(&self) -> &Arc<VersionInfo> {
        (**self).version_info()
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        (**self).inbox_id()
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext<Self::Db>> {
        (**self).context_ref()
    }

    fn mls_provider(&self) -> XmtpOpenMlsProvider<<Self::Db as XmtpDb>::Connection> {
        (**self).mls_provider()
    }

    async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Self>>, GroupError> {
        // Get inner groups
        let inner_result = (**self).sync_welcomes().await?;

        // Create new vector with the correct type
        let mut result = Vec::with_capacity(inner_result.len());

        // For each group in the result
        for group in inner_result {
            // Create a new MlsGroup with self as the client
            let new_group = MlsGroup::new(
                self.clone(),
                group.group_id.clone(),
                group.dm_id.clone(),
                group.created_at_ns,
            );
            result.push(new_group);
        }

        Ok(result)
    }

    async fn get_installation_diff(
        &self,
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
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
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        ClientError,
    > {
        (**self)
            .get_key_packages_for_installation_ids(installation_ids)
            .await
    }

    async fn get_association_state(
        &self,
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
        inbox_id: InboxIdRef<'_>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        (**self)
            .get_association_state(conn, inbox_id, to_sequence_id)
            .await
    }

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
        identifiers: &[(impl AsIdRef, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        (**self)
            .batch_get_association_state(conn, identifiers)
            .await
    }

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        (**self).query_group_messages(group_id, conn).await
    }
}

#[cfg(target_arch = "wasm32")]
impl<T> ScopedGroupClient for std::rc::Rc<T>
where
    T: ScopedGroupClient,
{
    type ApiClient = <T as ScopedGroupClient>::ApiClient;
    type Db = <T as ScopedGroupClient>::Db;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        (**self).api()
    }

    fn store(&self) -> &<T as ScopedGroupClient>::Db {
        (**self).store()
    }

    fn local_events(&self) -> &broadcast::Sender<LocalEvents> {
        (**self).local_events()
    }

    fn worker_handle(&self) -> Option<Arc<WorkerHandle<SyncMetric>>> {
        (**self).worker_handle()
    }

    fn version_info(&self) -> &Arc<VersionInfo> {
        (**self).version_info()
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        (**self).inbox_id()
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext<Self::Db>> {
        (**self).context_ref()
    }

    fn mls_provider(&self) -> XmtpOpenMlsProvider<<Self::Db as XmtpDb>::Connection> {
        (**self).mls_provider()
    }

    async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Self>>, GroupError> {
        // Get inner groups
        let inner_result = (**self).sync_welcomes().await?;

        // Create new vector with the correct type
        let mut result = Vec::with_capacity(inner_result.len());

        // For each group in the result
        for group in inner_result {
            // Create a new MlsGroup with self as the client
            let new_group = MlsGroup::new(
                self.clone(),
                group.group_id.clone(),
                group.dm_id.clone(),
                group.created_at_ns,
            );
            result.push(new_group);
        }

        Ok(result)
    }

    async fn get_installation_diff(
        &self,
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
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
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        ClientError,
    > {
        (**self)
            .get_key_packages_for_installation_ids(installation_ids)
            .await
    }

    async fn get_association_state(
        &self,
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
        inbox_id: InboxIdRef<'_>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        (**self)
            .get_association_state(conn, inbox_id, to_sequence_id)
            .await
    }

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
        identifiers: &[(impl AsIdRef, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        (**self)
            .batch_get_association_state(conn, identifiers)
            .await
    }

    async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection<<<T as ScopedGroupClient>::Db as XmtpDb>::Connection>,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        (**self).query_group_messages(group_id, conn).await
    }
}
