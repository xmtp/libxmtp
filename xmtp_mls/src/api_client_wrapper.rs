use std::collections::HashMap;

use xmtp_proto::{
    api_client::{Error as ApiError, ErrorKind, XmtpApiClient, XmtpMlsClient},
    xmtp::mls::message_contents::{
        group_message::{Version as GroupMessageVersion, V1 as GroupMessageV1},
        welcome_message::{Version as WelcomeMessageVersion, V1 as WelcomeMessageV1},
        WelcomeMessage as WelcomeMessageProto,
    },
    xmtp::{
        message_api::v3::{
            publish_welcomes_request::WelcomeMessageRequest, ConsumeKeyPackagesRequest,
            KeyPackageUpload, PublishToGroupRequest, PublishWelcomesRequest,
            RegisterInstallationRequest, UploadKeyPackagesRequest,
        },
        mls::message_contents::GroupMessage,
    },
};

pub struct WelcomeMessage {
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) installation_id: Vec<u8>,
}

type KeyPackageMap = HashMap<Vec<u8>, Vec<u8>>;

pub struct ApiClientWrapper<ApiClient> {
    api_client: ApiClient,
}

impl<ApiClient> ApiClientWrapper<ApiClient>
where
    ApiClient: XmtpMlsClient + XmtpApiClient,
{
    pub fn new(api_client: ApiClient) -> Self {
        Self { api_client }
    }

    pub async fn register_installation(
        &self,
        last_resort_key_package: &[u8],
    ) -> Result<Vec<u8>, ApiError> {
        let res = self
            .api_client
            .register_installation(RegisterInstallationRequest {
                last_resort_key_package: Some(KeyPackageUpload {
                    key_package_tls_serialized: last_resort_key_package.to_vec(),
                }),
            })
            .await?;

        Ok(res.installation_id)
    }

    pub async fn upload_key_packages(&self, key_packages: Vec<&[u8]>) -> Result<(), ApiError> {
        self.api_client
            .upload_key_packages(UploadKeyPackagesRequest {
                key_packages: key_packages
                    .into_iter()
                    .map(|kp| KeyPackageUpload {
                        key_package_tls_serialized: kp.to_vec(),
                    })
                    .collect(),
            })
            .await?;

        Ok(())
    }

    pub async fn consume_key_packages(
        &self,
        installation_ids: Vec<&[u8]>,
    ) -> Result<KeyPackageMap, ApiError> {
        let res = self
            .api_client
            .consume_key_packages(ConsumeKeyPackagesRequest {
                installation_ids: installation_ids.iter().map(|id| id.to_vec()).collect(),
            })
            .await?;

        if res.key_packages.len() != installation_ids.len() {
            println!("mismatched number of results");
            return Err(ApiError::new(ErrorKind::MlsError));
        }

        let mapping: KeyPackageMap = res
            .key_packages
            .into_iter()
            .enumerate()
            .map(|(idx, key_package)| {
                (
                    installation_ids[idx].to_vec(),
                    key_package.key_package_tls_serialized,
                )
            })
            .collect();

        Ok(mapping)
    }

    pub async fn publish_welcomes(
        &self,
        welcome_messages: Vec<WelcomeMessage>,
    ) -> Result<(), ApiError> {
        let welcome_requests: Vec<WelcomeMessageRequest> = welcome_messages
            .into_iter()
            .map(|msg| WelcomeMessageRequest {
                installation_id: msg.installation_id,
                welcome_message: Some(WelcomeMessageProto {
                    version: Some(WelcomeMessageVersion::V1(WelcomeMessageV1 {
                        welcome_message_tls_serialized: msg.ciphertext,
                    })),
                }),
            })
            .collect();

        self.api_client
            .publish_welcomes(PublishWelcomesRequest {
                welcome_messages: welcome_requests,
            })
            .await?;

        Ok(())
    }

    pub async fn publish_to_group(&self, group_messages: Vec<&[u8]>) -> Result<(), ApiError> {
        let to_send: Vec<GroupMessage> = group_messages
            .iter()
            .map(|msg| GroupMessage {
                version: Some(GroupMessageVersion::V1(GroupMessageV1 {
                    mls_message_tls_serialized: msg.to_vec(),
                })),
            })
            .collect();

        self.api_client
            .publish_to_group(PublishToGroupRequest { messages: to_send })
            .await?;

        Ok(())
    }
}
