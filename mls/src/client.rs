use crate::{
    group::Group, identity::Identity, openmls_rust_persistent_crypto::OpenMlsRustPersistentCrypto,
    utils::now_ns,
};
use openmls::{
    prelude::{
        GroupId, MlsGroup, MlsGroupConfig, MlsMessageIn, MlsMessageInBody, MlsMessageOut,
        SenderRatchetConfiguration, TlsSerializeTrait, PURE_CIPHERTEXT_WIRE_FORMAT_POLICY,
    },
    prelude_test::KeyPackage,
};
use openmls_traits::types::Ciphersuite;
use tls_codec::Deserialize;
use xmtp::types::networking::{Envelope, PublishRequest, QueryRequest, XmtpApiClient};
use xmtp_networking::grpc_api_helper::Client as ApiClient;
const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;
use uuid::Uuid;

const API_URL: &str = "https://dev.xmtp.network:5556";

pub struct Client {
    pub identity: Identity,
    pub crypto: OpenMlsRustPersistentCrypto,
    pub api_client: ApiClient,
    pub id: String,
}

impl Client {
    pub async fn create() -> Client {
        let id = Uuid::new_v4();
        println!("Client ID: {:?}", id.to_string());
        let crypto = OpenMlsRustPersistentCrypto::default();
        let identity = Identity::new(CIPHERSUITE, &crypto, id.to_string().as_bytes());
        let networking = ApiClient::create(API_URL.to_string(), true).await.unwrap();

        let client = Client {
            identity,
            crypto,
            api_client: networking,
            id: id.to_string(),
        };

        client.publish_key_packages().await;

        client
    }

    pub fn create_group(&self) -> Group {
        let group_id = Uuid::new_v4().to_string();
        println!("Creating group: {}", group_id);
        let mut group_aad = group_id.as_bytes().to_vec();
        // TODO: understand wtf this does
        group_aad.extend(b" AAD");

        let group_config = build_group_config();

        let mut mls_group = MlsGroup::new_with_group_id(
            &self.crypto,
            &self.identity.signer,
            &group_config,
            GroupId::from_slice(group_id.as_bytes()),
            self.identity.credential_with_key.clone(),
        )
        .expect("failed to create group");
        mls_group.set_aad(group_aad.as_slice());

        return Group::new(&self, mls_group, group_id);
    }

    pub async fn publish_key_packages(&self) {
        let key_packages = self.identity.kp.values().collect::<Vec<_>>();
        for kp in key_packages {
            self.api_client
                .publish(
                    "".to_string(),
                    PublishRequest {
                        envelopes: vec![Envelope {
                            timestamp_ns: now_ns(),
                            message: serde_json::to_string(&kp).unwrap().as_bytes().to_vec(),
                            content_topic: self.contact_topic(),
                        }],
                    },
                )
                .await
                .expect("failed to publish");
        }
    }

    pub async fn get_key_package(&self, contact_id: &str) -> Option<KeyPackage> {
        let query_response = self
            .api_client
            .query(QueryRequest {
                content_topics: vec![build_contact_topic(contact_id)],
                start_time_ns: 0,
                end_time_ns: 0,
                paging_info: None,
            })
            .await
            .unwrap();

        for envelope in query_response.envelopes {
            let kp: KeyPackage = serde_json::from_slice(&envelope.message).unwrap();
            return Some(kp);
        }

        None
    }

    pub async fn send_welcome(&self, member_id: &str, welcome: MlsMessageOut) {
        self.api_client
            .publish(
                "".to_string(),
                PublishRequest {
                    envelopes: vec![Envelope {
                        timestamp_ns: now_ns(),
                        message: welcome
                            .tls_serialize_detached()
                            .expect("serialization failed"),
                        content_topic: build_welcome_topic(member_id),
                    }],
                },
            )
            .await
            .expect("failed to send welcome");
    }

    pub async fn load_groups(&self) -> Vec<Group> {
        let query_response = self
            .api_client
            .query(QueryRequest {
                content_topics: vec![build_welcome_topic(self.id.as_str())],
                start_time_ns: 0,
                end_time_ns: 0,
                paging_info: None,
            })
            .await
            .unwrap();

        let mut groups: Vec<Group> = vec![];

        for env in query_response.envelopes {
            let msg: MlsMessageIn = MlsMessageIn::tls_deserialize(&mut env.message.as_slice())
                .expect("failed to deserialize")
                .into();
            match msg.extract() {
                MlsMessageInBody::Welcome(welcome) => {
                    let group_config = build_group_config();
                    let mut mls_group =
                        MlsGroup::new_from_welcome(&self.crypto, &group_config, welcome, None)
                            .expect("Failed to create MlsGroup");

                    let group_id = mls_group.group_id().to_vec();
                    // XXX: Use Welcome's encrypted_group_info field to store group_name.
                    let group_name = String::from_utf8(group_id.clone()).unwrap();
                    let group_aad = group_name.clone() + " AAD";

                    mls_group.set_aad(group_aad.as_bytes());
                    groups.push(Group::new(&self, mls_group, group_name));
                }
                _ => panic!("unexpected message type"),
            }
        }

        return groups;
    }

    fn contact_topic(&self) -> String {
        build_contact_topic(self.id.as_str())
    }
}

fn build_contact_topic(id: &str) -> String {
    format!("/xmtp/3/contact-{:?}/proto", id)
}

fn build_welcome_topic(id: &str) -> String {
    format!("/xmtp/3/welcome-{:?}/proto", id)
}

fn build_group_config() -> MlsGroupConfig {
    MlsGroupConfig::builder()
        .use_ratchet_tree_extension(true)
        // Allowing past epochs to be kept around, which weakens forward secrecy
        .max_past_epochs(8)
        .wire_format_policy(PURE_CIPHERTEXT_WIRE_FORMAT_POLICY)
        .sender_ratchet_configuration(SenderRatchetConfiguration::new(20, 1000))
        .build()
}

#[cfg(test)]
mod tests {
    use openmls::prelude::Member;

    use super::*;

    #[tokio::test]
    async fn test_client_create() {
        let client = Client::create().await;
        assert_eq!(client.identity.kp.len(), 1);

        let kp = client.get_key_package(client.id.as_str()).await;
        assert!(kp.is_some());
    }

    #[tokio::test]
    async fn create_group() {
        let client = Client::create().await;
        let group = client.create_group();
        assert_eq!(
            group.mls_group.group_id().as_slice(),
            group.group_id.as_bytes()
        );
    }

    #[tokio::test]
    async fn join_group() {
        let client_1 = Client::create().await;
        let client_2 = Client::create().await;
        let mut group = client_1.create_group();
        group.add_member(client_2.id.as_str()).await;
        assert_eq!(group.mls_group.members().collect::<Vec<Member>>().len(), 2);

        let client_2_groups = client_2.load_groups().await;
        assert_eq!(client_2_groups.len(), 1);
        let group_from_welcome = client_2_groups.get(0).unwrap();
        assert_eq!(group_from_welcome.group_id, group.group_id);
        assert_eq!(
            group_from_welcome
                .mls_group
                .members()
                .collect::<Vec<Member>>()
                .len(),
            2
        );
    }
}
