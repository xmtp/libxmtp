use std::collections::HashMap;

use corecrypto::encryption;
use protobuf;
use protobuf::Message;

mod conversation;
mod ethereum_utils;
mod invitation;
mod keys;
mod proto;
mod signature;
mod topic;
mod traits;
use invitation::InvitationV1;
use keys::{
    key_bundle::{PrivateKeyBundle, PublicKeyBundle, SignedPublicKeyBundle},
    public_key,
};

use conversation::{InvitationContext, TopicData};

use traits::WalletAssociated;

// Tests
mod test_lib;

pub struct Keystore {
    // Private key bundle powers most operations
    private_key_bundle: Option<PrivateKeyBundle>,
    // Topic Keys
    topic_keys: HashMap<String, TopicData>,

    num_sets: u32,
}

impl Keystore {
    // new() is a constructor for the Keystore struct
    pub fn new() -> Self {
        Keystore {
            // Empty option for private key bundle
            private_key_bundle: None,
            // Topic keys
            topic_keys: HashMap::new(),
            num_sets: 0,
        }
    }

    // == Keystore methods ==
    // Set private identity key from protobuf bytes
    pub fn set_private_key_bundle(&mut self, private_key_bundle: &[u8]) -> Result<(), String> {
        // Deserialize protobuf bytes into a SignedPrivateKey struct
        let private_key_result: protobuf::Result<proto::private_key::PrivateKeyBundle> =
            protobuf::Message::parse_from_bytes(private_key_bundle);
        if private_key_result.is_err() {
            return Err("could not parse private key bundle".to_string());
        }
        // Get the private key from the result
        let private_key = private_key_result.as_ref().unwrap();
        let private_key_bundle = private_key.v2();

        // If the deserialization was successful, set the privateIdentityKey field
        if private_key_result.is_ok() {
            self.private_key_bundle =
                Some(PrivateKeyBundle::from_proto(&private_key_bundle).unwrap());
            self.num_sets += 1;
            return Ok(());
        } else {
            return Err("could not parse private key bundle".to_string());
        }
    }

    // Process proto::keystore::DecryptV1Request
    pub fn decrypt_v1(&self, request_bytes: &[u8]) -> Result<Vec<u8>, String> {
        // Decode request bytes into proto::keystore::DecryptV1Request
        let request_result: protobuf::Result<proto::keystore::DecryptV1Request> =
            protobuf::Message::parse_from_bytes(request_bytes);
        if request_result.is_err() {
            return Err("could not parse decrypt v1 request".to_string());
        }
        let request = request_result.as_ref().unwrap();
        // Create a list of responses
        let mut responses = Vec::new();

        let private_key_bundle = self.private_key_bundle.as_ref().unwrap();

        // Iterate over the requests
        for request in &request.requests {
            let payload = &request.payload;
            let peer_keys = &request.peer_keys;
            let header_bytes = &request.header_bytes;
            let is_sender = &request.is_sender;

            let mut response = proto::keystore::decrypt_response::Response::new();

            // Extract XMTP-like X3DH secret
            let secret_result = private_key_bundle.derive_shared_secret_xmtp(
                &PublicKeyBundle::from_proto(&peer_keys)
                    .unwrap()
                    .to_fake_signed_public_key_bundle(),
                &private_key_bundle.pre_keys[0].public_key,
                !is_sender,
            );
            if secret_result.is_err() {
                return Err("could not derive shared secret".to_string());
            }
            let secret = secret_result.unwrap();

            let ciphertext = &payload.aes256_gcm_hkdf_sha256();

            let decrypt_result = encryption::decrypt(
                ciphertext.payload.as_slice(),
                ciphertext.hkdf_salt.as_slice(),
                ciphertext.gcm_nonce.as_slice(),
                &secret,
                Some(header_bytes.as_slice()),
            );

            match decrypt_result {
                Ok(decrypted) => {
                    let mut success_response =
                        proto::keystore::decrypt_response::response::Success::new();
                    success_response.decrypted = decrypted;
                    response.response = Some(
                        proto::keystore::decrypt_response::response::Response::Result(
                            success_response,
                        ),
                    );
                }
                Err(e) => {
                    let mut error_response = proto::keystore::KeystoreError::new();
                    error_response.message = e.to_string();

                    error_response.code = protobuf::EnumOrUnknown::new(
                        proto::keystore::ErrorCode::ERROR_CODE_UNSPECIFIED,
                    );
                    response.response = Some(
                        proto::keystore::decrypt_response::response::Response::Error(
                            error_response,
                        ),
                    );
                }
            }
            responses.push(response);
        }
        let mut response_proto = proto::keystore::DecryptResponse::new();
        response_proto.responses = responses;
        return Ok(response_proto.write_to_bytes().unwrap());
    }

    // Process proto::keystore::DecryptV2Request
    pub fn decrypt_v2(&self, request_bytes: &[u8]) -> Result<Vec<u8>, String> {
        // Decode request bytes into proto::keystore::DecryptV2Request
        let request_result: protobuf::Result<proto::keystore::DecryptV2Request> =
            protobuf::Message::parse_from_bytes(request_bytes);
        if request_result.is_err() {
            return Err("could not parse decrypt v2 request".to_string());
        }
        let request = request_result.unwrap();
        // Create a list of responses
        let mut responses = Vec::new();

        // For each request in the request list
        for request in request.requests {
            // TODO: validate the object

            // Extract the payload, headerBytes and contentTopic
            // const { payload, headerBytes, contentTopic } = req
            let payload = request.payload;
            let header_bytes = request.header_bytes;
            let content_topic = request.content_topic;

            // Try to get the topic data
            // const topicData = this.topicKeys.get(contentTopic)
            let topic_data = self.topic_keys.get(&content_topic);
            if topic_data.is_none() {
                // Error with the content_topic
                return Err("could not find topic data".to_string());
            }
            let topic_data = topic_data.unwrap();

            let ciphertext = payload.unwrap().aes256_gcm_hkdf_sha256().clone();

            // Try to decrypt the payload
            let decrypt_result = encryption::decrypt(
                ciphertext.payload.as_slice(),
                ciphertext.hkdf_salt.as_slice(),
                ciphertext.gcm_nonce.as_slice(),
                &topic_data.key,
                Some(header_bytes.as_slice()),
            );

            let mut response = proto::keystore::decrypt_response::Response::new();

            // If decryption was successful, return the decrypted payload
            // If decryption failed, return an error
            match decrypt_result {
                Ok(decrypted) => {
                    let mut success_response =
                        proto::keystore::decrypt_response::response::Success::new();
                    success_response.decrypted = decrypted;
                    response.response = Some(
                        proto::keystore::decrypt_response::response::Response::Result(
                            success_response,
                        ),
                    );
                }
                Err(e) => {
                    let mut error_response = proto::keystore::KeystoreError::new();
                    error_response.message = e;
                    error_response.code = protobuf::EnumOrUnknown::new(
                        proto::keystore::ErrorCode::ERROR_CODE_UNSPECIFIED,
                    );
                    response.response = Some(
                        proto::keystore::decrypt_response::response::Response::Error(
                            error_response,
                        ),
                    );
                }
            }
            responses.push(response);
        }
        let mut response_proto = proto::keystore::DecryptResponse::new();
        response_proto.responses = responses;
        return Ok(response_proto.write_to_bytes().unwrap());
    }

    fn get_conversation_from_topic(
        &self,
        topic: &str,
    ) -> Result<proto::keystore::ConversationReference, String> {
        let topic_result = self.topic_keys.get(topic);
        if topic_result.is_none() {
            return Err("could not find topic data".to_string());
        }
        // Finally, if we have the topic data then add success + conversation object
        let topic_data = topic_result.unwrap();
        let mut success_conversation = proto::keystore::ConversationReference::new();
        success_conversation.topic = topic.to_string();
        success_conversation.created_ns = topic_data.created;
        success_conversation.peer_address = topic_data.peer_address.clone();
        // Create invitation context from topic data context
        let mut invitation_context = proto::invitation::invitation_v1::Context::new();
        if topic_data.context.is_some() {
            let context = topic_data.context.as_ref().unwrap();
            invitation_context.conversation_id = context.conversation_id.clone();
            for (key, value) in context.metadata.iter() {
                invitation_context
                    .metadata
                    .insert(key.to_string(), value.to_string());
            }
            success_conversation.context = Some(invitation_context).into();
        }
        return Ok(success_conversation);
    }

    // Create invite
    pub fn create_invite(&mut self, request_bytes: &[u8]) -> Result<Vec<u8>, String> {
        // if no self.private_key_bundle, then return error
        if self.private_key_bundle.is_none() {
            return Err("no private key bundle".to_string());
        }
        // Decode request bytes into proto::keystore::CreateInviteRequest
        let invite_request_result = InvitationV1::invite_request_from_bytes(request_bytes);
        if invite_request_result.is_err() {
            return Err("could not parse invite request".to_string());
        }
        let invite_request = invite_request_result.unwrap();

        // Validate the request
        if invite_request.recipient.is_none() {
            return Err("missing recipient".to_string());
        }
        // Try parsing the recipient into a SignedPublicKeyBundle for validation
        let validation_parse_result =
            SignedPublicKeyBundle::from_proto(invite_request.recipient.as_ref().unwrap());
        if validation_parse_result.is_err() {
            return Err("Could not validate recipient bundle".to_string());
        }
        let recipient = invite_request.recipient.unwrap();

        // Create a random invitation
        let invitation = InvitationV1::create_random(invite_request.context);

        // Create a sealed invitation
        let mut sealed_invitation_header = proto::invitation::SealedInvitationHeaderV1::new();
        let self_private_key_ref = self.private_key_bundle.as_ref().unwrap();
        sealed_invitation_header.sender =
            Some(self_private_key_ref.signed_public_key_bundle_proto()).into();
        sealed_invitation_header.recipient = Some(recipient).into();
        sealed_invitation_header.created_ns = invite_request.created_ns;

        // Now seal the invitation with our self_private_key_ref
        let sealed_invitation_result =
            self_private_key_ref.seal_invitation(&sealed_invitation_header, &invitation);
        if sealed_invitation_result.is_err() {
            return Err("could not seal invitation".to_string());
        }

        let sealed_invitation = sealed_invitation_result.unwrap();

        // Add the conversation from the invite
        let save_result = self.save_invitation(&sealed_invitation.write_to_bytes().unwrap());
        if save_result.is_err() {
            return Err("could not save own created invitation".to_string());
        }
        let topic = save_result.unwrap();
        let conversation_result = self.get_conversation_from_topic(&topic);
        if conversation_result.is_err() {
            return Err("could not get conversation from topic".to_string());
        }

        // Create the response
        let mut response = proto::keystore::CreateInviteResponse::new();
        response.conversation = Some(conversation_result.unwrap()).into();
        response.payload = sealed_invitation.write_to_bytes().unwrap();

        return Ok(response.write_to_bytes().unwrap());
    }

    // Save invites keystore impl
    pub fn save_invites(&mut self, request_bytes: &[u8]) -> Result<Vec<u8>, String> {
        // Decode request bytes into proto::keystore::SaveInvitesRequest
        let request_result: protobuf::Result<proto::keystore::SaveInvitesRequest> =
            protobuf::Message::parse_from_bytes(request_bytes);
        if request_result.is_err() {
            return Err("could not parse save invites request".to_string());
        }
        let request = request_result.unwrap();

        let mut full_response = proto::keystore::SaveInvitesResponse::new();
        // For each request, process the sealed invite + other data to save a conversation
        for request in request.requests {
            let sealed_invitation_bytes = request.payload;
            let save_result = self.save_invitation(&sealed_invitation_bytes);
            let mut response = proto::keystore::save_invites_response::Response::new();
            if save_result.is_err() {
                let mut error_response = proto::keystore::KeystoreError::new();
                error_response.message = save_result.err().unwrap();
                error_response.code = protobuf::EnumOrUnknown::new(
                    proto::keystore::ErrorCode::ERROR_CODE_UNSPECIFIED,
                );
                response.response = Some(
                    proto::keystore::save_invites_response::response::Response::Error(
                        error_response,
                    ),
                );
                full_response.responses.push(response);
                continue;
            }
            // Do not use the request.content_topic as it's not tamper proof, instead use the
            // returned unsealed topic
            let unsealed_topic = save_result.unwrap();
            // Check if topic_keys has the content_topic
            let topic_data = self.topic_keys.get(unsealed_topic.as_str());
            // If not, then return an error
            if topic_data.is_none() {
                let mut error_response = proto::keystore::KeystoreError::new();
                error_response.message =
                    format!("could not find topic data for {}", request.content_topic);
                error_response.code = protobuf::EnumOrUnknown::new(
                    proto::keystore::ErrorCode::ERROR_CODE_UNSPECIFIED,
                );
                response.response = Some(
                    proto::keystore::save_invites_response::response::Response::Error(
                        error_response,
                    ),
                );
                full_response.responses.push(response);
                continue;
            }

            // Finally, if we have the topic data then add success + conversation object
            let topic_data = topic_data.unwrap();
            let mut success_conversation = proto::keystore::ConversationReference::new();
            success_conversation.topic = unsealed_topic;
            success_conversation.peer_address = topic_data.peer_address.clone();
            success_conversation.created_ns = topic_data.created;
            // Create invitation context from topic data context
            let mut invitation_context = proto::invitation::invitation_v1::Context::new();
            if topic_data.context.is_some() {
                let context = topic_data.context.as_ref().unwrap();
                invitation_context.conversation_id = context.conversation_id.clone();
                for (key, value) in context.metadata.iter() {
                    invitation_context
                        .metadata
                        .insert(key.to_string(), value.to_string());
                }
                success_conversation.context = Some(invitation_context).into();
            }
            let mut success = proto::keystore::save_invites_response::response::Success::new();
            success.conversation = Some(success_conversation).into();

            let success_response =
                proto::keystore::save_invites_response::response::Response::Result(success);
            response.response = Some(success_response);

            full_response.responses.push(response);
        }
        return Ok(full_response.write_to_bytes().unwrap());
    }

    // Save single invitation
    pub fn save_invitation(&mut self, sealed_invitation_bytes: &[u8]) -> Result<String, String> {
        // Check that self.private_key_bundle is set, otherwise return an error
        if self.private_key_bundle.is_none() {
            return Err("private key bundle not set yet".to_string());
        }

        // Deserialize invitation bytes into a protobuf::invitation::InvitationV1 struct
        let invitation_result = InvitationV1::sealed_invitation_from_bytes(sealed_invitation_bytes);
        if invitation_result.is_err() {
            return Err("could not parse invitation".to_string());
        }
        let invitation = invitation_result.unwrap();

        // Need to parse the header_bytes as protobuf::invitation::SealedInvitationHeaderV1
        let header_result: protobuf::Result<proto::invitation::SealedInvitationHeaderV1> =
            protobuf::Message::parse_from_bytes(&invitation.header_bytes);
        if header_result.is_err() {
            return Err("could not parse invitation header".to_string());
        }
        // Get the invitation header from the result
        let invitation_header = header_result.as_ref().unwrap();

        // Extract sender and recipient from invitation_header
        let recipient_bundle_proto = &invitation_header.recipient;
        let sender_bundle_proto = &invitation_header.sender;

        // Check if our public bundle equals
        let Ok(recipient_bundle) = SignedPublicKeyBundle::from_proto(recipient_bundle_proto.as_ref().unwrap()) else {
            return Err("Could not parse recipient bundle from v1 sealed invitation header".to_string());
        };
        let Ok(sender_bundle) = SignedPublicKeyBundle::from_proto(sender_bundle_proto.as_ref().unwrap()) else {
            return Err("Could not parse sender bundle from v1 sealed invitation header".to_string());
        };

        // TODO: STOPSHIP: must update signed_public_key_bundle to use/check signatures
        let is_sender = sender_bundle
            == self
                .private_key_bundle
                .as_ref()
                .unwrap()
                .signed_public_key_bundle();
        let Ok(peer_wallet_address) = (if is_sender {
            recipient_bundle.wallet_address()
        } else {
            sender_bundle.wallet_address()
        }) else {
            return Err("Could not get wallet address from peer bundle".to_string());
        };

        // Check the header time from the sealed invite
        // TODO: check header time from the sealed invite
        let header_time = invitation_header.created_ns;

        // Attempt to decrypt the invitation
        let decrypt_result = self
            .private_key_bundle
            .as_ref()
            .unwrap()
            .unseal_invitation(&invitation, &invitation_header);
        if decrypt_result.is_err() {
            return Err(format!(
                "could not decrypt invitation: {}",
                decrypt_result.err().unwrap()
            ));
        }
        // Get the decrypted invitation from the result
        let decrypted_invitation = decrypt_result.unwrap();

        // Encryption field should contain the key bytes
        let key_bytes = decrypted_invitation
            .aes256_gcm_hkdf_sha256()
            .key_material
            .as_slice();

        // Context field should contain conversationId
        let conversation_id = &decrypted_invitation.context.conversation_id;
        let mut context_fields = HashMap::new();
        // Iterate through metadata map and add to context_fields
        for key in decrypted_invitation.context.metadata.keys() {
            context_fields.insert(
                key.to_string(),
                decrypted_invitation.context.metadata[key].to_string(),
            );
        }

        let topic = &decrypted_invitation.topic;

        let optional_context = if decrypted_invitation.context.is_some() {
            Some(InvitationContext {
                conversation_id: conversation_id.to_string(),
                metadata: context_fields,
            })
        } else {
            None
        };

        self.topic_keys.insert(
            topic.to_string(),
            TopicData {
                key: key_bytes.to_vec(),
                peer_address: peer_wallet_address,
                // If the invitation has a context, then use the context, otherwise use None
                context: optional_context,
                created: header_time,
            },
        );

        return Ok(topic.to_string());
    }

    // Get serialized keystore.ConversationReference
    pub fn get_v2_conversations(&self) -> Result<Vec<Vec<u8>>, String> {
        let mut conversations = Vec::new();
        for (topic, topic_data) in self.topic_keys.iter() {
            let mut conversation = proto::keystore::ConversationReference::new();
            conversation.topic = topic.clone();
            conversation.created_ns = topic_data.created;
            if topic_data.context.is_some() {
                let context = topic_data.context.as_ref().unwrap();
                let mut invitation_context = proto::invitation::invitation_v1::Context::new();
                invitation_context.conversation_id = context.conversation_id.clone();
                for (key, value) in context.metadata.iter() {
                    invitation_context
                        .metadata
                        .insert(key.to_string(), value.to_string());
                }
                conversation.context = Some(invitation_context).into();
            }
            conversations.push(conversation.write_to_bytes().unwrap());
        }
        // Sort the conversations by created_ns
        conversations.sort_by(|a, b| {
            let a_conversation = proto::keystore::ConversationReference::parse_from_bytes(a)
                .unwrap()
                .created_ns;
            let b_conversation = proto::keystore::ConversationReference::parse_from_bytes(b)
                .unwrap()
                .created_ns;
            a_conversation.cmp(&b_conversation)
        });
        return Ok(conversations);
    }

    pub fn get_topic_key(&self, topic_id: &str) -> Option<Vec<u8>> {
        let topic_data = self.topic_keys.get(topic_id);
        if topic_data.is_none() {
            return None;
        }
        return Some(topic_data.unwrap().key.clone());
    }

    fn create_unspecified_keystore_err(message: &str) -> proto::keystore::KeystoreError {
        let mut error_response = proto::keystore::KeystoreError::new();
        error_response.message = message.to_string();

        error_response.code =
            protobuf::EnumOrUnknown::new(proto::keystore::ErrorCode::ERROR_CODE_UNSPECIFIED);
        return error_response;
    }

    // Process proto::keystore::EncryptV1Request
    pub fn encrypt_v1(&self, request_bytes: &[u8]) -> Result<Vec<u8>, String> {
        // Decode request bytes into proto::keystore::EncryptV1Request
        let request_result: protobuf::Result<proto::keystore::EncryptV1Request> =
            protobuf::Message::parse_from_bytes(request_bytes);
        if request_result.is_err() {
            return Err("could not parse encrypt v1 request".to_string());
        }
        let request = request_result.as_ref().unwrap();
        // Create a list of responses
        let mut responses = Vec::new();

        let private_key_bundle = self.private_key_bundle.as_ref().unwrap();

        // Iterate over the requests
        for request in &request.requests {
            let mut response = proto::keystore::encrypt_response::Response::new();

            // Extract recipient, payload, header_bytes
            // assert that they're not empty otherwise log error and continue
            if request.recipient.is_none() {
                response.response = Some(
                    proto::keystore::encrypt_response::response::Response::Error(
                        Keystore::create_unspecified_keystore_err(
                            "Missing recipient in encrypt request",
                        ),
                    ),
                );
                responses.push(response);
                continue;
            }
            let recipient = request.recipient.as_ref().unwrap();
            let payload = request.payload.as_ref();
            let header_bytes = request.header_bytes.as_ref();

            // TODO: STOPSHIP: hack: massage the recipient PublicKeyBundle into a fake SignedPublicKeyBundle
            // so that we can use the existing sharedSecret function
            let public_key_bundle_result = PublicKeyBundle::from_proto(&recipient);
            if public_key_bundle_result.is_err() {
                response.response = Some(
                    proto::keystore::encrypt_response::response::Response::Error(
                        Keystore::create_unspecified_keystore_err("Could not parse recipient"),
                    ),
                );
                responses.push(response);
                continue;
            }
            let public_key_bundle = public_key_bundle_result.unwrap();
            let signed_public_key_bundle = public_key_bundle.to_fake_signed_public_key_bundle();

            // Extract XMTP-like X3DH secret
            let secret_result = private_key_bundle.derive_shared_secret_xmtp(
                &signed_public_key_bundle,
                &private_key_bundle.pre_keys[0].public_key,
                false, // sender is doing the encrypting
            );
            if secret_result.is_err() {
                response.response = Some(
                    proto::keystore::encrypt_response::response::Response::Error(
                        Keystore::create_unspecified_keystore_err(
                            &secret_result.as_ref().err().unwrap(),
                        ),
                    ),
                );
                responses.push(response);
                continue;
            }
            let secret = secret_result.unwrap();

            // Encrypt the payload
            let encrypt_result = encryption::encrypt(&payload, &secret, Some(&header_bytes));

            match encrypt_result {
                Ok(encrypted) => {
                    // TODO: this can be modularized away
                    let mut success_response =
                        proto::keystore::encrypt_response::response::Success::new();
                    let mut aes256_gcm_hkdf_sha256 =
                        proto::ciphertext::ciphertext::Aes256gcmHkdfsha256::new();
                    aes256_gcm_hkdf_sha256.payload = encrypted.payload;
                    aes256_gcm_hkdf_sha256.hkdf_salt = encrypted.hkdf_salt;
                    aes256_gcm_hkdf_sha256.gcm_nonce = encrypted.gcm_nonce;
                    let mut ciphertext = proto::ciphertext::Ciphertext::new();
                    ciphertext.set_aes256_gcm_hkdf_sha256(aes256_gcm_hkdf_sha256);
                    success_response.encrypted = Some(ciphertext).into();
                    response.response = Some(
                        proto::keystore::encrypt_response::response::Response::Result(
                            success_response,
                        ),
                    );
                }
                Err(e) => {
                    let mut error_response = proto::keystore::KeystoreError::new();
                    error_response.message = e.to_string();

                    error_response.code = protobuf::EnumOrUnknown::new(
                        proto::keystore::ErrorCode::ERROR_CODE_UNSPECIFIED,
                    );
                    response.response = Some(
                        proto::keystore::encrypt_response::response::Response::Error(
                            error_response,
                        ),
                    );
                }
            }
            responses.push(response);
        }
        let mut response_proto = proto::keystore::EncryptResponse::new();
        response_proto.responses = responses;
        return Ok(response_proto.write_to_bytes().unwrap());
    }

    // Process proto::keystore::EncryptV2Request
    pub fn encrypt_v2(&self, request_bytes: &[u8]) -> Result<Vec<u8>, String> {
        // Decode request bytes into proto::keystore::EncryptV2Request
        let request_result: protobuf::Result<proto::keystore::EncryptV2Request> =
            protobuf::Message::parse_from_bytes(request_bytes);
        if request_result.is_err() {
            return Err("could not parse encrypt v2 request".to_string());
        }
        let request = request_result.unwrap();
        // Create a list of responses
        let mut responses = Vec::new();

        // For each request in the request list
        for request in request.requests {
            // TODO: validate the object

            // Extract the payload, headerBytes and contentTopic
            // const { payload, headerBytes, contentTopic } = req
            let payload = request.payload.as_ref();
            let header_bytes = request.header_bytes;
            let content_topic = request.content_topic;

            // Try to get the topic data
            // const topicData = this.topicKeys.get(contentTopic)
            let topic_data = self.topic_keys.get(&content_topic);
            if topic_data.is_none() {
                // Error with the content_topic
                return Err("could not find topic data".to_string());
            }
            let topic_data = topic_data.unwrap();

            // Try to encrypt the payload
            let encrypt_result =
                encryption::encrypt(payload, &topic_data.key, Some(header_bytes.as_slice()));

            let mut response = proto::keystore::encrypt_response::Response::new();

            // If encryption was successful, return the encrypted payload
            // If encryption failed, return an error
            match encrypt_result {
                Ok(encrypted) => {
                    let mut success_response =
                        proto::keystore::encrypt_response::response::Success::new();
                    let mut aes256_gcm_hkdf_sha256 =
                        proto::ciphertext::ciphertext::Aes256gcmHkdfsha256::new();
                    aes256_gcm_hkdf_sha256.payload = encrypted.payload;
                    aes256_gcm_hkdf_sha256.hkdf_salt = encrypted.hkdf_salt;
                    aes256_gcm_hkdf_sha256.gcm_nonce = encrypted.gcm_nonce;
                    let mut ciphertext = proto::ciphertext::Ciphertext::new();
                    ciphertext.set_aes256_gcm_hkdf_sha256(aes256_gcm_hkdf_sha256);
                    success_response.encrypted = Some(ciphertext).into();
                    response.response = Some(
                        proto::keystore::encrypt_response::response::Response::Result(
                            success_response,
                        ),
                    );
                }
                Err(e) => {
                    let mut error_response = proto::keystore::KeystoreError::new();
                    error_response.message = e;
                    error_response.code = protobuf::EnumOrUnknown::new(
                        proto::keystore::ErrorCode::ERROR_CODE_UNSPECIFIED,
                    );
                    response.response = Some(
                        proto::keystore::encrypt_response::response::Response::Error(
                            error_response,
                        ),
                    );
                }
            }
            responses.push(response);
        }
        let mut response_proto = proto::keystore::EncryptResponse::new();
        response_proto.responses = responses;
        return Ok(response_proto.write_to_bytes().unwrap());
    }

    pub fn get_public_key_bundle(&self) -> Result<Vec<u8>, String> {
        if self.private_key_bundle.is_none() {
            return Err("public key bundle is none".to_string());
        }
        // Go from private_key_bundle to public_key_bundle
        let public_key_bundle = self
            .private_key_bundle
            .as_ref()
            .unwrap()
            .signed_public_key_bundle_proto();
        return Ok(public_key_bundle.write_to_bytes().unwrap());
    }

    pub fn get_account_address(&self) -> Result<String, String> {
        if self.private_key_bundle.is_none() {
            return Err("private key bundle is none".to_string());
        }
        self.private_key_bundle.as_ref().unwrap().eth_address()
    }
    // == end keystore api ==
}
