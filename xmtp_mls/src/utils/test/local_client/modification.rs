use super::*;
use xmtp_proto::identity::api::v1::prelude::GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response;

pub enum ModificationType {
    FetchKeyPackages(Box<dyn Fn(&mut FetchKeyPackagesResponse) + Send + Sync>),
    QueryGroupMessages(Box<dyn Fn(&mut QueryGroupMessagesResponse) + Send + Sync>),
    QueryWelcomeMessages(Box<dyn Fn(&mut QueryWelcomeMessagesResponse) + Send + Sync>),
    GetIdentityUpdates(Box<dyn Fn(&mut GetIdentityUpdatesV2Response) + Send + Sync>),
    GetInboxIds(Box<dyn Fn(&mut GetInboxIdsResponse) + Send + Sync>),
    VerifyScwSignatures(
        Box<dyn Fn(&mut VerifySmartContractWalletSignaturesResponse) + Send + Sync>,
    ),
    NextStreamedMessage(Box<dyn Fn(&mut GroupMessage) + Send + Sync>),
    NextStreamedWelcome(Box<dyn Fn(&mut WelcomeMessage) + Send + Sync>),
}

impl ModificationType {
    pub(super) fn fetch_kps(self) -> Box<dyn Fn(&mut FetchKeyPackagesResponse) + Send + Sync> {
        match self {
            Self::FetchKeyPackages(f) => f,
            _ => panic!("does not exist"),
        }
    }

    pub(super) fn query_group_messages(
        self,
    ) -> Box<dyn Fn(&mut QueryGroupMessagesResponse) + Send + Sync> {
        match self {
            Self::QueryGroupMessages(f) => f,
            _ => panic!("does not exist"),
        }
    }

    pub(super) fn query_welcome_messages(
        self,
    ) -> Box<dyn Fn(&mut QueryWelcomeMessagesResponse) + Send + Sync> {
        match self {
            Self::QueryWelcomeMessages(f) => f,
            _ => panic!("does not exist"),
        }
    }

    pub(super) fn get_identity_updates(
        self,
    ) -> Box<dyn Fn(&mut GetIdentityUpdatesV2Response) + Send + Sync> {
        match self {
            Self::GetIdentityUpdates(f) => f,
            _ => panic!("does not exist"),
        }
    }

    pub(super) fn get_inbox_ids(self) -> Box<dyn Fn(&mut GetInboxIdsResponse) + Send + Sync> {
        match self {
            Self::GetInboxIds(f) => f,
            _ => panic!("does not exist"),
        }
    }

    pub(super) fn verify_scw_signatures(
        self,
    ) -> Box<dyn Fn(&mut VerifySmartContractWalletSignaturesResponse) + Send + Sync> {
        match self {
            Self::VerifyScwSignatures(f) => f,
            _ => panic!("does not exist"),
        }
    }

    pub(super) fn next_streamed_message(self) -> Box<dyn Fn(&mut GroupMessage) + Send + Sync> {
        match self {
            Self::NextStreamedMessage(f) => f,
            _ => panic!("does not exist"),
        }
    }

    pub(super) fn next_streamed_welcome(self) -> Box<dyn Fn(&mut WelcomeMessage) + Send + Sync> {
        match self {
            Self::NextStreamedWelcome(f) => f,
            _ => panic!("does not exist"),
        }
    }
}

impl std::fmt::Debug for ModificationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", "modification")
    }
}

impl LocalTestClient {
    pub fn get_mod(&self, res: &'static str) -> Option<ModificationType> {
        let mut m = self.modification.lock();
        if let Some(modification) = m.get_mut(res) {
            return modification.pop();
        } else {
            None
        }
    }

    /// Modifies the next response sent by the backend
    pub fn modify_fetch_key_packages<
        F: Fn(&mut FetchKeyPackagesResponse) + Send + Sync + 'static,
    >(
        &self,
        fun: F,
    ) {
        let mut m = self.modification.lock();
        m.entry(std::any::type_name::<FetchKeyPackagesResponse>())
            .or_insert_with(Vec::new)
            .push(ModificationType::FetchKeyPackages(Box::new(fun)));
    }

    /// Modifies the next response sent by the backend
    pub fn modify_query_group_messages<
        F: Fn(&mut QueryGroupMessagesResponse) + Send + Sync + 'static,
    >(
        &self,
        fun: F,
    ) {
        let mut m = self.modification.lock();
        m.entry(std::any::type_name::<QueryGroupMessagesResponse>())
            .or_insert_with(Vec::new)
            .push(ModificationType::QueryGroupMessages(Box::new(fun)));
    }

    /// Modifies the next response sent by the backend
    pub fn modify_query_welcome_messages<
        F: Fn(&mut QueryWelcomeMessagesResponse) + Send + Sync + 'static,
    >(
        &self,
        fun: F,
    ) {
        let mut m = self.modification.lock();
        m.entry(std::any::type_name::<QueryWelcomeMessagesResponse>())
            .or_insert_with(Vec::new)
            .push(ModificationType::QueryWelcomeMessages(Box::new(fun)));
    }

    /// Modifies the next response sent by the backend
    pub fn modify_get_identity_updates_v2<
        F: Fn(&mut GetIdentityUpdatesV2Response) + Send + Sync + 'static,
    >(
        &self,
        fun: F,
    ) {
        let mut m = self.modification.lock();
        m.entry(std::any::type_name::<GetIdentityUpdatesV2Response>())
            .or_insert_with(Vec::new)
            .push(ModificationType::GetIdentityUpdates(Box::new(fun)));
    }

    /// Modifies the next response sent by the backend
    pub fn modify_get_inbox_ids<F: Fn(&mut GetInboxIdsResponse) + Sync + Send + 'static>(
        &self,
        fun: F,
    ) {
        let mut m = self.modification.lock();
        m.entry(std::any::type_name::<GetInboxIdsResponse>())
            .or_insert_with(Vec::new)
            .push(ModificationType::GetInboxIds(Box::new(fun)));
    }

    /// Modifies the next response sent by the backend
    pub fn modify_verify_scw_signatures<
        F: Fn(&mut VerifySmartContractWalletSignaturesResponse) + Sync + Send + 'static,
    >(
        &self,
        fun: F,
    ) {
        let mut m = self.modification.lock();
        m.entry(std::any::type_name::<
            VerifySmartContractWalletSignaturesResponse,
        >())
        .or_insert_with(Vec::new)
        .push(ModificationType::VerifyScwSignatures(Box::new(fun)));
    }

    /// Modifies the next response sent by the backend
    pub fn modify_next_streamed_message<F: Fn(&mut GroupMessage) + Sync + Send + 'static>(
        &self,
        fun: F,
    ) {
        let mut m = self.modification.lock();
        m.entry(std::any::type_name::<GroupMessage>())
            .or_insert_with(Vec::new)
            .push(ModificationType::NextStreamedMessage(Box::new(fun)));
    }

    /// Modifies the next response sent by the backend
    pub fn modify_next_streamed_welcome<F: Fn(&mut WelcomeMessage) + Sync + Send + 'static>(
        &self,
        fun: F,
    ) {
        let mut m = self.modification.lock();
        m.entry(std::any::type_name::<WelcomeMessage>())
            .or_insert_with(Vec::new)
            .push(ModificationType::NextStreamedWelcome(Box::new(fun)));
    }
}
