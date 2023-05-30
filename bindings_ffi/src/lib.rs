// use async_trait::async_trait;
// use xmtp::{
//     networking::XmtpApiClient, persistence::in_memory_persistence::InMemoryPersistence,
//     storage::EncryptedMessageStore,
// };
// use xmtp_cryptography::utils::LocalWallet;
// use xmtp_networking::grpc_api_helper::{self, Subscription};
// use xmtp_proto::xmtp::message_api::v1::{Envelope, PagingInfo, PublishResponse, QueryResponse};
//
// pub type FfiXmtpClient = xmtp::Client<FfiApiClient, InMemoryPersistence, EncryptedMessageStore>;
uniffi::include_scaffolding!("xmtpv3");

fn add(a: u32, b: u32) -> u32 {
    a + b
}

// #[swift_bridge::bridge]
// mod ffi {
//     extern "Rust" {
//         type LocalWallet;
//         type FfiXmtpClient;
//
//         async fn create_client(
//             wallet: LocalWallet,
//             host: &str,
//             is_secure: bool,
//         ) -> Result<FfiXmtpClient, String>;
//     }
// }
//
// async fn create_client(
//     owner: LocalWallet,
//     host: &str,
//     is_secure: bool,
//     // TODO proper error handling
// ) -> Result<xmtp::Client<FfiApiClient, InMemoryPersistence, EncryptedMessageStore>, String> {
//     let api_client = FfiApiClient::new(host, is_secure).await?;
//
//     let mut xmtp_client = xmtp::ClientBuilder::new(owner.into())
//         .api_client(api_client)
//         .build()
//         .map_err(|e| format!("{:?}", e))?;
//     xmtp_client.init().await.map_err(|e| e.to_string())?;
//     Ok(xmtp_client)
// }
//
// pub struct FfiApiClient {
//     client: grpc_api_helper::Client,
// }
//
// impl Default for FfiApiClient {
//     fn default() -> Self {
//         //TODO: Remove once Default constraint lifted from clientBuilder
//         unimplemented!()
//     }
// }
//
// impl FfiApiClient {
//     async fn new(host: &str, is_secure: bool) -> Result<Self, String> {
//         let client = grpc_api_helper::Client::create(host.to_string(), is_secure)
//             .await
//             .map_err(|e| format!("{}", e))?;
//
//         Ok(Self { client })
//     }
// }
//
// #[async_trait]
// impl XmtpApiClient for FfiApiClient {
//     async fn publish(
//         &mut self,
//         token: String,
//         envelopes: Vec<Envelope>,
//         // TODO: use error enums
//     ) -> Result<PublishResponse, String> {
//         self.client
//             .publish(token, envelopes)
//             .await
//             .map_err(|e| format!("{}", e))
//     }
//
//     async fn query(
//         &self,
//         topic: String,
//         start_time: Option<u64>,
//         end_time: Option<u64>,
//         paging_info: Option<PagingInfo>,
//         // TODO: use error enums
//     ) -> Result<QueryResponse, String> {
//         self.client
//             .query(topic, start_time, end_time, paging_info)
//             .await
//             .map_err(|e| format!("{}", e))
//     }
//
//     async fn subscribe(&mut self, topics: Vec<String>) -> Result<Subscription, String> {
//         self.client
//             .subscribe(topics)
//             .await
//             .map_err(|e| format!("{}", e))
//     }
// }
//
// #[cfg(test)]
// mod tests {
//     use std::time::{SystemTime, UNIX_EPOCH};
//
//     use uuid::Uuid;
//     use xmtp::networking::XmtpApiClient;
//     use xmtp_cryptography::{utils::rng, utils::LocalWallet};
//
//     static ADDRESS: &str = "http://localhost:5556";
//
//     fn test_envelope(topic: String) -> super::Envelope {
//         let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
//
//         super::Envelope {
//             timestamp_ns: time_since_epoch.as_nanos() as u64,
//             content_topic: topic,
//             message: vec![65],
//         }
//     }
//
//     // Try a query on a test topic, and make sure we get a response
//     #[tokio::test]
//     async fn test_publish_query() {
//         let wallet = LocalWallet::new(&mut rng());
//         let mut client = super::create_client(wallet, ADDRESS, false).await.unwrap();
//         let topic = Uuid::new_v4();
//         client
//             .api_client
//             .publish("".to_string(), vec![test_envelope(topic.to_string())])
//             .await
//             .unwrap();
//
//         let result = client
//             .api_client
//             .query(topic.to_string(), None, None, None)
//             .await
//             .unwrap();
//
//         let envelopes = result.envelopes;
//         assert_eq!(envelopes.len(), 1);
//
//         let first_envelope = envelopes.get(0).unwrap();
//         assert_eq!(first_envelope.content_topic, topic.to_string());
//         assert!(first_envelope.timestamp_ns > 0);
//         assert!(!first_envelope.message.is_empty());
//     }
//
//     #[tokio::test]
//     async fn test_subscribe() {
//         let wallet = LocalWallet::new(&mut rng());
//         let mut client = super::create_client(wallet, ADDRESS, false).await.unwrap();
//         let topic = Uuid::new_v4();
//         let mut sub = client
//             .api_client
//             .subscribe(vec![topic.to_string()])
//             .await
//             .unwrap();
//         tokio::time::sleep(std::time::Duration::from_millis(100)).await;
//         client
//             .api_client
//             .publish("".to_string(), vec![test_envelope(topic.to_string())])
//             .await
//             .unwrap();
//         tokio::time::sleep(std::time::Duration::from_millis(200)).await;
//
//         let messages = sub.get_messages();
//         assert_eq!(messages.len(), 1);
//         let messages = sub.get_messages();
//         assert_eq!(messages.len(), 0);
//
//         sub.close_stream();
//         assert!(sub.is_closed());
//     }
// }
//
