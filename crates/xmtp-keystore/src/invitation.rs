use super::proto;
use super::topic::Topic;

use base64::{engine::general_purpose, Engine as _};
use protobuf::{Message, MessageField};
use rand::{CryptoRng, RngCore};

pub struct InvitationV1 {}

// Contains a bunch of static methods to process invitations
impl InvitationV1 {
    pub fn sealed_invitation_from_bytes(
        bytes: &[u8],
    ) -> Result<proto::invitation::SealedInvitationV1, String> {
        // Deserialize invitation bytes into a protobuf::invitation::InvitationV1 struct
        let invitation_result: protobuf::Result<proto::invitation::SealedInvitation> =
            protobuf::Message::parse_from_bytes(bytes);
        if invitation_result.is_err() {
            return Err("could not parse invitation".to_string());
        }
        // Get the invitation from the result
        let sealed_invitation = invitation_result.as_ref().unwrap();
        // TODO: revisit clone, better to move out if possible
        return Ok(sealed_invitation.v1().clone());
    }

    // TODO: a bit weird to stick it here, will move when we have a better idea of the structure
    pub fn invite_request_from_bytes(
        bytes: &[u8],
    ) -> Result<proto::keystore::CreateInviteRequest, String> {
        // Deserialize bytes in to proto::keystore::CreateInviteRequest
        let create_invite_request_result: protobuf::Result<proto::keystore::CreateInviteRequest> =
            protobuf::Message::parse_from_bytes(bytes);
        if create_invite_request_result.is_err() {
            return Err("could not parse create invite request".to_string());
        }
        // Get the invitation from the result
        let create_invite_request = create_invite_request_result.as_ref().unwrap();
        // TODO: revisit clone, better to move out if possible
        return Ok(create_invite_request.clone());
    }

    pub fn create_random(
        context: MessageField<proto::invitation::invitation_v1::Context>,
    ) -> proto::invitation::InvitationV1 {
        // Create a random invitation
        let mut invitation = proto::invitation::InvitationV1::new();
        let mut random_bytes_buffer = [0u8; 32];
        // Fill bytes with thread_rng which  implements CryptoRng marker trait
        rand::thread_rng().fill_bytes(&mut random_bytes_buffer);
        // Generate 32 random bytes, then base64 encode, then remove trailing =, then replace / with -
        let mut random_bytes_b64 = general_purpose::STANDARD
            .encode(&random_bytes_buffer)
            .replace("/", "-");
        random_bytes_b64 = random_bytes_b64.trim_end_matches('=').to_string();
        // Build topic
        let topic = Topic::build_direct_message_topic_v2(&random_bytes_b64);

        let mut key_material_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key_material_bytes);
        let mut aes256_gcm_hkdf_sha256 =
            proto::invitation::invitation_v1::Aes256gcmHkdfsha256::new();
        aes256_gcm_hkdf_sha256.key_material = key_material_bytes.to_vec();
        invitation.set_aes256_gcm_hkdf_sha256(aes256_gcm_hkdf_sha256);
        invitation.topic = topic;
        invitation.context = context;
        return invitation;
    }
}
