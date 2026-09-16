mod publish;
pub use publish::Publish;
mod query;
pub use query::Query;
mod query_newest;
pub use query_newest::QueryNewest;
mod get_inbox_ids;
pub use get_inbox_ids::GetInboxIds;
mod get_configuration;
pub use get_configuration::{GET_CONFIGURATION_PATH, GetConfiguration};
mod verify_smart_contract_wallet_signatures;
pub use verify_smart_contract_wallet_signatures::VerifySmartContractWalletSignatures;
mod subscribe_static;
pub use subscribe_static::SubscribeStatic;
mod register;
pub use register::Register;
mod unregister;
pub use unregister::Unregister;
mod update_subscriptions;
pub use update_subscriptions::UpdateSubscriptions;

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_proto::api::Endpoint;

    #[xmtp_common::test(unwrap_try = true)]
    fn endpoint_paths_match_backend_services() {
        let paths = [
            (
                Publish(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.PublishService/Publish",
            ),
            (
                Query(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.QueryService/Query",
            ),
            (
                QueryNewest(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.QueryService/QueryNewest",
            ),
            (
                GetInboxIds(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.IdentityService/GetInboxIds",
            ),
            (
                GetConfiguration(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.ConfigurationService/GetConfiguration",
            ),
            (
                VerifySmartContractWalletSignatures(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.IdentityService/VerifySmartContractWalletSignatures",
            ),
            (
                SubscribeStatic(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.SubscriptionService/SubscribeStatic",
            ),
            (
                Register(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.NotificationService/Register",
            ),
            (
                Unregister(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.NotificationService/Unregister",
            ),
            (
                UpdateSubscriptions(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.NotificationService/UpdateSubscriptions",
            ),
        ];
        for (actual, expected) in paths {
            assert_eq!(actual, expected);
        }
    }
}
