use std::{future::Future, sync::Arc};

use xmtp_id::{associations::AssociationState, scw_verifier::SmartContractSignatureVerifier};
use xmtp_proto::{
    api_client::{trait_impls::XmtpApi, Error},
    xmtp::mls::api::v1::GroupMessage,
};

use crate::{
    api::ApiClientWrapper,
    client::{ClientError, MessageProcessingError, XmtpMlsLocalContext},
    identity_updates::{InstallationDiff, InstallationDiffError},
    storage::{refresh_state::EntityKind, DbConnection, EncryptedMessageStore},
    verified_key_package_v2::VerifiedKeyPackageV2,
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Client,
};

use super::group_membership::{GroupMembership, MembershipDiff};

#[trait_variant::make(ScopedGroupClient: Send)]
pub trait LocalScopedGroupClient: Send + Sync + Sized {
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
        group_id: &Vec<u8>,
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError>;

    async fn process_for_id<Fut, ProcessingFn, ReturnValue>(
        &self,
        entity_id: &Vec<u8>,
        entity_kind: EntityKind,
        cursor: u64,
        process_envelope: ProcessingFn,
    ) -> Result<ReturnValue, MessageProcessingError>
    where
        Fut: Send + Future<Output = Result<ReturnValue, MessageProcessingError>>,
        ProcessingFn: Send + FnOnce(XmtpOpenMlsProvider) -> Fut;
}

impl<ApiClient, Verifier> ScopedGroupClient for Client<ApiClient, Verifier>
where
    ApiClient: XmtpApi + Clone,
    Verifier: SmartContractSignatureVerifier + Clone,
{
    type ApiClient = ApiClient;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        &self.api_client
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext> {
        self.context()
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

    async fn get_association_state(
        &self,
        conn: &DbConnection,
        inbox_id: String,
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

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection,
        identifiers: &[(String, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        crate::Client::<ApiClient, Verifier>::batch_get_association_state(self, conn, identifiers)
            .await
    }

    async fn query_group_messages(
        &self,
        group_id: &Vec<u8>,
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        crate::Client::<ApiClient, Verifier>::query_group_messages(self, group_id, conn).await
    }

    async fn process_for_id<Fut, ProcessingFn, ReturnValue>(
        &self,
        entity_id: &Vec<u8>,
        entity_kind: EntityKind,
        cursor: u64,
        process_envelope: ProcessingFn,
    ) -> Result<ReturnValue, MessageProcessingError>
    where
        Fut: Send + Future<Output = Result<ReturnValue, MessageProcessingError>>,
        ProcessingFn: Send + FnOnce(XmtpOpenMlsProvider) -> Fut,
    {
        crate::Client::process_for_id(self, entity_id, entity_kind, cursor, process_envelope).await
    }
}
/*
impl<T> ScopedGroupClient for &T
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

    fn inbox_id(&self) -> String {
        (**self).inbox_id()
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext> {
        (**self).context_ref()
    }

    fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, ClientError> {
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

    async fn get_association_state(
        &self,
        conn: &DbConnection,
        inbox_id: String,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        (**self)
            .get_association_state(conn, inbox_id, to_sequence_id)
            .await
    }

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection,
        identifiers: &[(String, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        (**self)
            .batch_get_association_state(conn, identifiers)
            .await
    }

    async fn query_group_messages(
        &self,
        group_id: &Vec<u8>,
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        (**self).query_group_messages(group_id, conn).await
    }

    async fn process_for_id<Fut, ProcessingFn, ReturnValue>(
        &self,
        entity_id: &Vec<u8>,
        entity_kind: EntityKind,
        cursor: u64,
        process_envelope: ProcessingFn,
    ) -> Result<ReturnValue, MessageProcessingError>
    where
        Fut: Send + Future<Output = Result<ReturnValue, MessageProcessingError>>,
        ProcessingFn: Send + FnOnce(XmtpOpenMlsProvider) -> Fut,
    {
        (**self)
            .process_for_id(entity_id, entity_kind, cursor, process_envelope)
            .await
    }
}
*/
impl<T> ScopedGroupClient for Arc<T>
where
    T: ScopedGroupClient + Send,
{
    type ApiClient = <T as ScopedGroupClient>::ApiClient;

    fn api(&self) -> &ApiClientWrapper<Self::ApiClient> {
        (**self).api()
    }

    fn store(&self) -> &EncryptedMessageStore {
        (**self).store()
    }

    fn inbox_id(&self) -> String {
        (**self).inbox_id()
    }

    fn context_ref(&self) -> &Arc<XmtpMlsLocalContext> {
        (**self).context_ref()
    }

    fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, ClientError> {
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

    async fn get_association_state(
        &self,
        conn: &DbConnection,
        inbox_id: String,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        (**self)
            .get_association_state(conn, inbox_id, to_sequence_id)
            .await
    }

    async fn batch_get_association_state(
        &self,
        conn: &DbConnection,
        identifiers: &[(String, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        (**self)
            .batch_get_association_state(conn, identifiers)
            .await
    }

    async fn query_group_messages(
        &self,
        group_id: &Vec<u8>,
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        (**self).query_group_messages(group_id, conn).await
    }

    async fn process_for_id<Fut, ProcessingFn, ReturnValue>(
        &self,
        entity_id: &Vec<u8>,
        entity_kind: EntityKind,
        cursor: u64,
        process_envelope: ProcessingFn,
    ) -> Result<ReturnValue, MessageProcessingError>
    where
        Fut: Send + Future<Output = Result<ReturnValue, MessageProcessingError>>,
        ProcessingFn: Send + FnOnce(XmtpOpenMlsProvider) -> Fut,
    {
        (**self)
            .process_for_id(entity_id, entity_kind, cursor, process_envelope)
            .await
    }
}
