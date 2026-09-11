mod publish;
pub use publish::Publish;
mod query;
pub use query::Query;
mod query_newest;
pub use query_newest::QueryNewest;
mod get_inbox_ids;
pub use get_inbox_ids::GetInboxIds;
mod verify_smart_contract_wallet_signatures;
pub use verify_smart_contract_wallet_signatures::VerifySmartContractWalletSignatures;
mod subscribe_static;
pub use subscribe_static::SubscribeStatic;

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
                VerifySmartContractWalletSignatures(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.IdentityService/VerifySmartContractWalletSignatures",
            ),
            (
                SubscribeStatic(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.SubscriptionService/SubscribeStatic",
            ),
        ];
        for (actual, expected) in paths {
            assert_eq!(actual, expected);
        }
    }
}
