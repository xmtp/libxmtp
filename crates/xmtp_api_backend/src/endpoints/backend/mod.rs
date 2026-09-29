mod publish;
pub use publish::Publish;
mod create_upload;
pub use create_upload::{CREATE_UPLOAD_PATH, CreateUpload};
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
    use prost::Message;
    use std::collections::HashSet;
    use xmtp_proto::api::Endpoint;

    #[xmtp_common::test(unwrap_try = true)]
    fn endpoint_paths_match_backend_services() {
        let paths = [
            (
                Publish(Default::default()).grpc_endpoint(),
                "/xmtp.backend.v1.PublishService/Publish",
            ),
            (
                CreateUpload(Default::default()).grpc_endpoint(),
                CREATE_UPLOAD_PATH,
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
        let descriptors = prost_types::FileDescriptorSet::decode(xmtp_proto::FILE_DESCRIPTOR_SET)?;
        let service_paths = descriptors
            .file
            .iter()
            .flat_map(|file| {
                let package = file.package.as_deref().unwrap_or_default();
                file.service.iter().flat_map(move |service| {
                    let service_name = service.name.as_deref().unwrap_or_default();
                    service.method.iter().map(move |method| {
                        format!(
                            "/{package}.{service_name}/{}",
                            method.name.as_deref().unwrap_or_default()
                        )
                    })
                })
            })
            .collect::<HashSet<_>>();
        for (actual, expected) in paths {
            assert_eq!(actual, expected);
            assert!(
                service_paths.contains(expected),
                "missing proto service path: {expected}"
            );
        }
    }
}
