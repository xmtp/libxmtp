use super::proto;

use protobuf::Message;

pub struct Invitation {}

// Contains a bunch of static methods to process invitations
impl Invitation {
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
        return Ok(sealed_invitation.v1().clone());
    }
}
