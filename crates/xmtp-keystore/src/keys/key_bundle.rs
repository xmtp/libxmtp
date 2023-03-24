use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::PublicKey;
use sha3::{Digest, Keccak256};
use corecrypto::encryption;

use super::super::{ecdh, proto};
use super::private_key::SignedPrivateKey;
use super::public_key;

use crate::ecdh::ECDHDerivable;

pub struct PrivateKeyBundle {
    // Underlying protos
    private_key_bundle_proto: proto::private_key::PrivateKeyBundleV2,

    pub identity_key: SignedPrivateKey,
    pub pre_keys: Vec<SignedPrivateKey>,
}

impl PrivateKeyBundle {
    pub fn from_proto(
        private_key_bundle: &proto::private_key::PrivateKeyBundleV2,
    ) -> Result<PrivateKeyBundle, String> {
        // Check if secp256k1 is available
        if !private_key_bundle.identity_key.has_secp256k1() {
            return Err("No secp256k1 key found".to_string());
        }

        let identity_key_result =
            SignedPrivateKey::from_proto(&private_key_bundle.identity_key.as_ref().unwrap());
        if identity_key_result.is_err() {
            return Err(identity_key_result.err().unwrap().to_string());
        }

        let pre_keys = private_key_bundle
            .pre_keys
            .iter()
            .map(|pre_key| SignedPrivateKey::from_proto(pre_key))
            .collect::<Result<Vec<SignedPrivateKey>, String>>()?;

        return Ok(PrivateKeyBundle {
            private_key_bundle_proto: private_key_bundle.clone(),
            identity_key: identity_key_result.unwrap(),
            pre_keys: pre_keys,
        });
    }

    pub fn eth_wallet_address_from_public_key(public_key_bytes: &[u8]) -> Result<String, String> {
        // Hash the public key bytes
        let mut hasher = Keccak256::new();
        hasher.update(public_key_bytes);
        let result = hasher.finalize();
        // Return the result as hex string, take the last 20 bytes
        return Ok(format!("0x{}", hex::encode(&result[12..])));
    }

    pub fn eth_address(&self) -> Result<String, String> {
        // Get the public key bytes
        let binding = self.identity_key.public_key.to_encoded_point(false);
        let public_key_bytes = binding.as_bytes();
        return PrivateKeyBundle::eth_wallet_address_from_public_key(public_key_bytes);
    }

    pub fn find_pre_key(&self, my_pre_key: PublicKey) -> Option<SignedPrivateKey> {
        for pre_key in self.pre_keys.iter() {
            if pre_key.public_key == my_pre_key {
                return Some(pre_key.clone());
            }
        }
        return None;
    }

    pub fn public_key_bundle(&self) -> PublicKeyBundle {
        let identity_key = self.identity_key.public_key;
        let pre_keys = self
            .pre_keys
            .iter()
            .map(|pre_key| pre_key.public_key)
            .collect::<Vec<_>>();

        return PublicKeyBundle {
            public_key_bundle_proto: proto::public_key::PublicKeyBundle::new(),
            identity_key: Some(identity_key),
            pre_key: Some(pre_keys[0]),
        };
    }

    // XMTP X3DH-like scheme for invitation decryption
    pub fn derive_shared_secret_xmtp(
        &self,
        peer_bundle: &SignedPublicKeyBundle,
        my_prekey: &dyn ecdh::ECDHKey,
        is_recipient: bool,
    ) -> Result<Vec<u8>, String> {
        let pre_key = self
            .find_pre_key(my_prekey.get_public_key())
            .ok_or("could not find prekey in private key bundle".to_string())?;
        let dh1: Vec<u8>;
        let dh2: Vec<u8>;
        // (STOPSHIP) TODO: better error handling
        // Get the private key bundle
        if is_recipient {
            dh1 = pre_key.shared_secret(&peer_bundle.identity_key).unwrap();
            dh2 = self
                .identity_key
                .shared_secret(&peer_bundle.pre_key)
                .unwrap();
        } else {
            dh1 = self
                .identity_key
                .shared_secret(&peer_bundle.pre_key)
                .unwrap();
            dh2 = pre_key.shared_secret(&peer_bundle.identity_key).unwrap();
        }
        let dh3 = pre_key.shared_secret(&peer_bundle.pre_key).unwrap();
        let secret = [dh1, dh2, dh3].concat();
        return Ok(secret);
    }

    pub fn unseal_invitation(
        &self,
        sealed_invitation: &proto::invitation::SealedInvitationV1,
        sealed_invitation_header: &proto::invitation::SealedInvitationHeaderV1,
    ) -> Result<proto::invitation::InvitationV1, String> {
        // Parse public key bundles from sealed_invitation header
        let sender_public_key_bundle =
            SignedPublicKeyBundle::from_proto(&sealed_invitation_header.sender).unwrap();
        let recipient_public_key_bundle =
            SignedPublicKeyBundle::from_proto(&sealed_invitation_header.recipient).unwrap();

        let secret: Vec<u8>;
        // reference our own identity key
        let viewer_identity_key = &self.identity_key;
        if viewer_identity_key.public_key == sender_public_key_bundle.identity_key {
            let secret_result = self.derive_shared_secret_xmtp(
                &recipient_public_key_bundle,
                &sender_public_key_bundle.pre_key,
                false,
            );
            if secret_result.is_err() {
                return Err("could not derive shared secret".to_string());
            }
            secret = secret_result.unwrap();
        } else {
            let secret_result = self.derive_shared_secret_xmtp(
                &sender_public_key_bundle,
                &recipient_public_key_bundle.pre_key,
                true,
            );
            if secret_result.is_err() {
                return Err("could not derive shared secret".to_string());
            }
            secret = secret_result.unwrap();
        }

        // Unwrap ciphertext
        let ciphertext = sealed_invitation.ciphertext.aes256_gcm_hkdf_sha256();

        let hkdf_salt = &ciphertext.hkdf_salt;
        let gcm_nonce = &ciphertext.gcm_nonce;
        let payload = &ciphertext.payload;

        // Try decrypting the invitation
        let decrypt_result = encryption::decrypt_v1(
            payload,
            hkdf_salt,
            gcm_nonce,
            &secret,
            Some(&sealed_invitation.header_bytes),
        );
        if decrypt_result.is_err() {
            return Err("could not decrypt invitation".to_string());
        }
        let decrypted_bytes = decrypt_result.unwrap();

        // Deserialize invitation bytes into a protobuf::invitation::InvitationV1 struct
        let invitation_result: protobuf::Result<proto::invitation::InvitationV1> =
            protobuf::Message::parse_from_bytes(&decrypted_bytes);
        if invitation_result.is_err() {
            return Err("could not parse invitation from decrypted bytes".to_string());
        }
        // Get the invitation from the result
        let invitation = invitation_result.as_ref().unwrap();
        return Ok(invitation.clone());
    }
}

pub struct PublicKeyBundle {
    // Underlying protos
    public_key_bundle_proto: proto::public_key::PublicKeyBundle,

    pub identity_key: Option<PublicKey>,
    pub pre_key: Option<PublicKey>,
}

impl PublicKeyBundle {
    pub fn from_proto(
        public_key_bundle: &proto::public_key::PublicKeyBundle,
    ) -> Result<PublicKeyBundle, String> {
        let mut identity_key: Option<PublicKey> = None;
        let mut pre_key: Option<PublicKey> = None;
        let identity_key_result =
            public_key::public_key_from_proto(public_key_bundle.identity_key.as_ref().unwrap());
        if identity_key_result.is_ok() {
            identity_key = Some(identity_key_result.unwrap());
        }

        let pre_key_result =
            public_key::public_key_from_proto(public_key_bundle.pre_key.as_ref().unwrap());
        if pre_key_result.is_ok() {
            pre_key = Some(pre_key_result.unwrap());
        }

        return Ok(PublicKeyBundle {
            public_key_bundle_proto: public_key_bundle.clone(),
            identity_key: identity_key,
            pre_key: pre_key,
        });
    }
}

pub struct SignedPublicKeyBundle {
    // Underlying protos
    signed_public_key_bundle_proto: proto::public_key::SignedPublicKeyBundle,

    pub identity_key: PublicKey,
    pub pre_key: PublicKey,
    // TODO: keep signature information
}

impl SignedPublicKeyBundle {
    pub fn from_proto(
        signed_public_key_bundle: &proto::public_key::SignedPublicKeyBundle,
    ) -> Result<SignedPublicKeyBundle, String> {
        // Check identity_key is populated
        if signed_public_key_bundle.identity_key.is_none() {
            return Err("No identity key found".to_string());
        }

        // Derive public key from SignedPublicKey
        let identity_key_result = public_key::signed_public_key_from_proto(
            signed_public_key_bundle.identity_key.as_ref().unwrap(),
        );
        if identity_key_result.is_err() {
            return Err(identity_key_result.err().unwrap().to_string());
        }
        let identity_key = identity_key_result.unwrap();

        // Check pre_key is populated
        if signed_public_key_bundle.pre_key.is_none() {
            return Err("No pre key found".to_string());
        }
        let pre_key_result = public_key::signed_public_key_from_proto(
            signed_public_key_bundle.pre_key.as_ref().unwrap(),
        );
        if pre_key_result.is_err() {
            return Err(pre_key_result.err().unwrap().to_string());
        }
        let pre_key = pre_key_result.unwrap();
        return Ok(SignedPublicKeyBundle {
            signed_public_key_bundle_proto: signed_public_key_bundle.clone(),
            identity_key: identity_key,
            pre_key: pre_key,
        });
    }
}
