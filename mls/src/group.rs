use openmls::prelude::{
    Credential, MlsGroup, MlsMessageIn, MlsMessageInBody, MlsMessageOut, ProcessedMessageContent,
};
use openmls::prelude::{ProtocolMessage, TlsSerializeTrait};
use std::str;
use tls_codec::Deserialize;
use xmtp::types::networking::{
    Envelope, PagingInfo, PublishRequest, QueryRequest, SortDirection, XmtpApiClient,
};

use crate::{client::Client, utils::now_ns};

pub struct ConversationMessage {
    pub user: String,
    pub message: String,
}

impl ConversationMessage {
    pub fn new(message: String, user: String) -> Self {
        Self { user, message }
    }
}

pub struct Group<'c> {
    pub group_id: String,
    pub mls_group: MlsGroup,
    pub client: &'c Client,
    pub messages: Vec<ConversationMessage>,
}

impl<'c> Group<'c> {
    pub fn new(client: &'c Client, mls_group: MlsGroup, group_id: String) -> Self {
        Self {
            client,
            mls_group,
            group_id,
            messages: vec![],
        }
    }

    pub async fn add_member(&mut self, member_id: &str) {
        let kp_result = self.client.get_key_package(member_id).await;
        match kp_result {
            Some(kp) => {
                let (out_messages, welcome, _group_info) = self
                    .mls_group
                    .add_members(&self.client.crypto, &self.client.identity.signer, &vec![kp])
                    .expect("failed to add member");

                self.client.send_welcome(member_id, welcome).await;
                self.publish(out_messages).await;
                self.mls_group
                    .merge_pending_commit(&self.client.crypto)
                    .expect("failed to merge pending commit");
            }
            None => panic!("failed to get key package for id {}", member_id),
        }
    }

    pub async fn publish(&self, message: MlsMessageOut) {
        self.client
            .publish(
                self.topic(),
                message
                    .tls_serialize_detached()
                    .expect("serialization failed"),
            )
            .await;
    }

    pub async fn send(&mut self, message: String) {
        let message_out = self
            .mls_group
            .create_message(
                &self.client.crypto,
                &self.client.identity.signer,
                message.as_bytes(),
            )
            .expect("failed to create message");

        self.publish(message_out).await;
        self.add_message(ConversationMessage::new(message, self.client.id.clone()));
    }

    pub async fn recv(&mut self) {
        let envelopes = self.client.query(self.topic()).await;

        for env in envelopes {
            let msg: MlsMessageIn = MlsMessageIn::tls_deserialize(&mut env.message.as_slice())
                .expect("failed to deserialize")
                .into();

            match msg.extract() {
                MlsMessageInBody::Welcome(_welcome) => {
                    panic!("unexpected welcome message in conversation topic")
                }
                MlsMessageInBody::PrivateMessage(pm) => {
                    let protocol_message: ProtocolMessage = pm.into();
                    let processed_message = self
                        .mls_group
                        .process_message(&self.client.crypto, protocol_message);
                    match processed_message {
                        Err(error) => {
                            // We need to ignore errors here since your own commits can't be processed twice
                            println!("error processing message: {:?}", error);
                            continue;
                        }
                        Ok(processed_message) => {
                            let processed_message_credential: Credential =
                                processed_message.credential().clone();

                            let content = processed_message.into_content();

                            self.process_message_content(content, processed_message_credential);
                        }
                    }
                }
                _ => panic!("unexpected message type"),
            }
        }
    }

    fn process_message_content(
        &mut self,
        content: ProcessedMessageContent,
        credential: Credential,
    ) {
        match content {
            ProcessedMessageContent::ApplicationMessage(application_message) => {
                let sender = credential.identity();
                let sender_name = str::from_utf8(sender).unwrap();
                let conversation_message = ConversationMessage::new(
                    String::from_utf8(application_message.into_bytes())
                        .unwrap()
                        .clone(),
                    sender_name.to_string(),
                );
                self.add_message(conversation_message);
            }
            ProcessedMessageContent::StagedCommitMessage(commit_ptr) => {
                self.mls_group
                    .merge_staged_commit(&self.client.crypto, *commit_ptr)
                    .expect("failed to process staged commit");
                // TODO: Handle the user being removed from a convo
            }
            _ => panic!("unexpected message content"),
        }
    }

    pub fn topic(&self) -> String {
        format!("/xmtp/3/group-{:?}/proto", self.group_id)
    }

    fn add_message(&mut self, message: ConversationMessage) {
        self.messages.push(message);
    }
}

#[cfg(test)]
mod tests {
    use openmls::prelude::Member;

    use super::*;

    #[tokio::test]
    async fn test_add_member() {
        let client_1 = Client::create().await;
        let client_2 = Client::create().await;

        let mut group = client_1.create_group();
        assert_eq!(group.mls_group.members().collect::<Vec<Member>>().len(), 1);
        group.add_member(client_2.id.as_str()).await;
        assert_eq!(group.mls_group.members().collect::<Vec<Member>>().len(), 2);
    }

    #[test_log::test(tokio::test)]
    async fn test_send_multiple_clients() {
        let client_1 = Client::create().await;
        let client_2 = Client::create().await;
        let client_3 = Client::create().await;

        let mut group = client_1.create_group();
        println!("adding second member");
        group.add_member(client_2.id.as_str()).await;
        group.send("hello world".to_string()).await;

        for mut client_2_group in client_2.load_groups().await {
            client_2_group.recv().await;
            assert_eq!(client_2_group.messages.len(), 1);
            assert_eq!(
                client_2_group
                    .mls_group
                    .members()
                    .collect::<Vec<Member>>()
                    .len(),
                2
            );
            println!("adding third member");
            client_2_group.add_member(client_3.id.as_str()).await;
            client_2_group.send("hello to you".to_string()).await;
        }

        group.recv().await;
        assert_eq!(group.messages.len(), 2);
        assert_eq!(group.mls_group.members().collect::<Vec<Member>>().len(), 3);

        for mut client_3_group in client_3.load_groups().await {
            client_3_group.recv().await;
            // Should only have the message sent after it was added
            assert_eq!(client_3_group.messages.len(), 1);
            assert_eq!(
                client_3_group
                    .mls_group
                    .members()
                    .collect::<Vec<Member>>()
                    .len(),
                3
            );
        }
    }
}
