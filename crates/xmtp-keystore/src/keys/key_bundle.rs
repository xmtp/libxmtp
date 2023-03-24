use k256::PublicKey;

use crate::keys::{private_key::SignedPrivateKey, public_key};
use crate::{
    encryption,
    ethereum_utils::{EthereumCompatibleKey, EthereumUtils},
    proto,
    public_key::SignedPublicKey,
};

use crate::traits::{
    BridgeSignableVersion, Buffable, ECDHDerivable, Sha256SignatureVerifier, SignedECDHKey,
    VerifiablePublicKeyBundle, WalletAssociated,
};

use protobuf::Message;

pub struct PrivateKeyBundle {
    // Underlying protos
    private_key_bundle_proto: proto::private_key::PrivateKeyBundleV2,

    pub identity_key: SignedPrivateKey,
    pub pre_keys: Vec<SignedPrivateKey>,
}

pub struct SignedPrivateKeyBundle {
    // Same as PrivateKeyBundle but with Signatures
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

    pub fn eth_address(&self) -> Result<String, String> {
        // Introspect proto and get the identity_key as a SignedPublicKey, then take the signature
        // and recover the wallet address
        let public_identity_key = self
            .private_key_bundle_proto
            .identity_key
            .public_key
            .as_ref()
            .unwrap()
            .clone();

        let xmtp_sig_request_bytes =
            EthereumUtils::xmtp_identity_key_payload(public_identity_key.key_bytes.as_slice());
        let personal_sign_message =
            EthereumUtils::get_personal_sign_message(xmtp_sig_request_bytes.as_slice());
        let eth_address_result = public_key::recover_wallet_public_key(
            &personal_sign_message,
            &public_identity_key.signature,
        );
        if eth_address_result.is_err() {
            return Err(eth_address_result.err().unwrap().to_string());
        }

        let recovered_wallet_key = eth_address_result.unwrap();
        return Ok(recovered_wallet_key.get_ethereum_address());
    }

    pub fn find_pre_key(&self, my_pre_key: PublicKey) -> Option<SignedPrivateKey> {
        for pre_key in self.pre_keys.iter() {
            if pre_key.public_key.to_unsigned() == my_pre_key {
                return Some(pre_key.clone());
            }
        }
        return None;
    }

    // Just shuffles the SignedPublicKeys out of the underlying proto and returns them in a
    // SignedPublicKeyBundle
    // TODO: STOPSHIP: need to include real signatures here
    pub fn signed_public_key_bundle_proto(&self) -> proto::public_key::SignedPublicKeyBundle {
        let mut signed_public_key_bundle_proto = proto::public_key::SignedPublicKeyBundle::new();
        // Use SignedPublicKey types for both identity_key and pre_key
        signed_public_key_bundle_proto.identity_key = Some(
            self.private_key_bundle_proto
                .identity_key
                .public_key
                .as_ref()
                .unwrap()
                .clone(),
        )
        .into();
        signed_public_key_bundle_proto.pre_key = Some(
            self.private_key_bundle_proto.pre_keys[0]
                .public_key
                .as_ref()
                .unwrap()
                .clone(),
        )
        .into();

        return signed_public_key_bundle_proto;
    }

    // Shuffle into a SignedPublicKeyBundle
    // TODO: STOPSHIP: need to include real signatures here
    pub fn signed_public_key_bundle(&self) -> SignedPublicKeyBundle {
        // TODO: this is empty for now, cannot stay that way
        let signed_public_key_bundle_proto = proto::public_key::SignedPublicKeyBundle::new();

        let mut pre_keys = Vec::new();
        // Iterate through the pre_keys and call '.public_key' on each
        for pre_key in self.pre_keys.iter() {
            pre_keys.push(pre_key.public_key.clone());
        }

        return SignedPublicKeyBundle {
            signed_public_key_bundle_proto: signed_public_key_bundle_proto,
            identity_key: self.identity_key.public_key.clone(),
            pre_key: pre_keys[0].clone(),
        };
    }

    // XMTP X3DH-like scheme for invitation decryption
    pub fn derive_shared_secret_xmtp(
        &self,
        peer_bundle: &impl VerifiablePublicKeyBundle<SignedPublicKey, SignedPublicKey>,
        my_prekey: &impl SignedECDHKey,
        is_recipient: bool,
    ) -> Result<Vec<u8>, String> {
        let dh1: Vec<u8>;
        let dh2: Vec<u8>;

        // Check bundle binding on peer_bundle
        match peer_bundle.verify_bundle_binding() {
            Ok(_) => {}
            Err(e) => {
                return Err(format!("could not verify bundle binding: {}", e));
            }
        }

        let pre_key = self
            .find_pre_key(my_prekey.get_public_key())
            .ok_or("could not find prekey in private key bundle".to_string())?;

        // (STOPSHIP) TODO: better error handling
        // Get the private key bundle
        if is_recipient {
            dh1 = pre_key
                .shared_secret(&peer_bundle.get_identity_key())
                .unwrap();
            dh2 = self
                .identity_key
                .shared_secret(&peer_bundle.get_prekey())
                .unwrap();
        } else {
            dh1 = self
                .identity_key
                .shared_secret(&peer_bundle.get_prekey())
                .unwrap();
            dh2 = pre_key
                .shared_secret(&peer_bundle.get_identity_key())
                .unwrap();
        }
        let dh3 = pre_key.shared_secret(&peer_bundle.get_prekey()).unwrap();
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
        // Reference our own identity key
        let viewer_identity_key = &self.identity_key;
        let is_local_sender =
            viewer_identity_key.public_key == sender_public_key_bundle.identity_key;

        // Check that one of the public key bundles is ours
        if !is_local_sender
            && viewer_identity_key.public_key != recipient_public_key_bundle.identity_key
        {
            return Err("Neither public key bundle is ours".to_string());
        }

        let local_public_key_bundle = if is_local_sender {
            &sender_public_key_bundle
        } else {
            &recipient_public_key_bundle
        };
        let remote_public_key_bundle = if is_local_sender {
            &recipient_public_key_bundle
        } else {
            &sender_public_key_bundle
        };

        let secret_result = self.derive_shared_secret_xmtp(
            remote_public_key_bundle,
            &local_public_key_bundle.pre_key,
            !is_local_sender,
        );
        secret = match secret_result {
            Ok(secret) => secret,
            Err(e) => {
                return Err(format!("could not derive shared secret: {}", e));
            }
        };

        // Unwrap ciphertext
        let ciphertext = sealed_invitation.ciphertext.aes256_gcm_hkdf_sha256();

        let hkdf_salt = &ciphertext.hkdf_salt;
        let gcm_nonce = &ciphertext.gcm_nonce;
        let payload = &ciphertext.payload;

        // Try decrypting the invitation
        let decrypt_result = encryption::decrypt(
            payload,
            hkdf_salt,
            gcm_nonce,
            &secret,
            Some(&sealed_invitation.header_bytes),
        );
        if decrypt_result.is_err() {
            return Err(format!(
                "could not decrypt invitation with error: {:?}",
                decrypt_result.err()
            ));
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

    pub fn seal_invitation(
        &self,
        sealed_invitation_header: &proto::invitation::SealedInvitationHeaderV1,
        invitation: &proto::invitation::InvitationV1,
    ) -> Result<proto::invitation::SealedInvitation, String> {
        // Parse public key bundles from sealed_invitation header
        let sender_public_key_bundle =
            SignedPublicKeyBundle::from_proto(&sealed_invitation_header.sender).unwrap();
        let recipient_public_key_bundle =
            SignedPublicKeyBundle::from_proto(&sealed_invitation_header.recipient).unwrap();

        let secret: Vec<u8>;
        // Reference our own identity key
        let viewer_identity_key = &self.identity_key;
        let is_local_sender =
            viewer_identity_key.public_key == sender_public_key_bundle.identity_key;
        let local_public_key_bundle = if is_local_sender {
            &sender_public_key_bundle
        } else {
            &recipient_public_key_bundle
        };
        let remote_public_key_bundle = if is_local_sender {
            &recipient_public_key_bundle
        } else {
            &sender_public_key_bundle
        };
        let secret_result = self.derive_shared_secret_xmtp(
            remote_public_key_bundle,
            &local_public_key_bundle.pre_key,
            !is_local_sender,
        );
        secret = match secret_result {
            Ok(secret) => secret,
            Err(e) => {
                return Err(format!("could not derive shared secret: {}", e));
            }
        };

        // Serialize invitation into bytes
        let invitation_bytes = invitation.write_to_bytes().unwrap();
        let header_bytes = sealed_invitation_header.write_to_bytes().unwrap();

        // Encrypt invitation bytes
        let ciphertext_result =
            encryption::encrypt(&invitation_bytes, &secret, Some(&header_bytes));
        if ciphertext_result.is_err() {
            return Err("could not encrypt invitation".to_string());
        }
        let ciphertext: encryption::Ciphertext = ciphertext_result.unwrap();

        // Convert ciphertext to protobuf::encryption::Ciphertext
        let mut ciphertext_aes256_gcm_hkdf_sha256 =
            proto::ciphertext::ciphertext::Aes256gcmHkdfsha256::new();
        ciphertext_aes256_gcm_hkdf_sha256.hkdf_salt = ciphertext.hkdf_salt;
        ciphertext_aes256_gcm_hkdf_sha256.gcm_nonce = ciphertext.gcm_nonce;
        ciphertext_aes256_gcm_hkdf_sha256.payload = ciphertext.payload;
        let mut ciphertext_proto = proto::ciphertext::Ciphertext::new();
        ciphertext_proto.set_aes256_gcm_hkdf_sha256(ciphertext_aes256_gcm_hkdf_sha256);

        // New SealedInvitation
        let mut sealed_invitation = proto::invitation::SealedInvitationV1::new();
        sealed_invitation.header_bytes = header_bytes;
        sealed_invitation.ciphertext = Some(ciphertext_proto).into();

        // Wrap it in the SealedInvitation proto message
        let mut sealed_invitation_proto = proto::invitation::SealedInvitation::new();
        sealed_invitation_proto.set_v1(sealed_invitation);
        return Ok(sealed_invitation_proto);
    }
}

impl WalletAssociated for SignedPrivateKeyBundle {
    fn wallet_address(&self) -> Result<String, String> {
        // Introspect proto and get the identity_key as a SignedPublicKey, then take the signature
        // and recover the wallet address
        let public_identity_key = self
            .private_key_bundle_proto
            .identity_key
            .public_key
            .as_ref()
            .unwrap()
            .clone();

        let xmtp_sig_request_bytes =
            EthereumUtils::xmtp_identity_key_payload(public_identity_key.key_bytes.as_slice());
        let personal_sign_message =
            EthereumUtils::get_personal_sign_message(xmtp_sig_request_bytes.as_slice());
        let eth_address_result = public_key::recover_wallet_public_key(
            &personal_sign_message,
            &public_identity_key.signature,
        );
        if eth_address_result.is_err() {
            return Err(eth_address_result.err().unwrap().to_string());
        }

        let recovered_wallet_key = eth_address_result.unwrap();
        return Ok(recovered_wallet_key.get_ethereum_address());
    }
}

pub struct PublicKeyBundle {
    pub identity_key: Option<PublicKey>,
    pub pre_key: Option<PublicKey>,
}

impl PublicKeyBundle {
    pub fn from_proto(
        public_key_bundle: &proto::public_key::PublicKeyBundle,
    ) -> Result<PublicKeyBundle, String> {
        let mut identity_key: Option<PublicKey> = None;
        let mut pre_key: Option<PublicKey> = None;
        // Check if identity key is set
        let identity_key_field = public_key_bundle.identity_key.as_ref();
        if identity_key_field.is_none() {
            return Err("Missing identity key in PublicKeyBundle".to_string());
        }
        let identity_key_result = public_key::public_key_from_proto(identity_key_field.unwrap());
        if identity_key_result.is_ok() {
            identity_key = Some(identity_key_result.unwrap());
        }

        let pre_key_field = public_key_bundle.pre_key.as_ref();
        if pre_key_field.is_none() {
            return Err("Missing pre key in PublicKeyBundle".to_string());
        }

        let pre_key_result = public_key::public_key_from_proto(pre_key_field.unwrap());
        if pre_key_result.is_ok() {
            pre_key = Some(pre_key_result.unwrap());
        }

        return Ok(PublicKeyBundle {
            identity_key: identity_key,
            pre_key: pre_key,
        });
    }

    pub fn to_fake_signed_public_key_bundle(&self) -> SignedPublicKeyBundle {
        return SignedPublicKeyBundle {
            signed_public_key_bundle_proto: proto::public_key::SignedPublicKeyBundle::new(),
            identity_key: self.identity_key.as_ref().unwrap().to_signed(),
            pre_key: self.pre_key.as_ref().unwrap().to_signed(),
        };
    }
}

pub struct SignedPublicKeyBundle {
    // Underlying protos
    signed_public_key_bundle_proto: proto::public_key::SignedPublicKeyBundle,

    pub identity_key: public_key::SignedPublicKey,
    pub pre_key: public_key::SignedPublicKey,
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
        let identity_key_result = public_key::signed_public_key_from_proto_v2(
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
        let pre_key_result = public_key::signed_public_key_from_proto_v2(
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

impl Buffable for SignedPublicKeyBundle {
    // TODO: cannot continue to rely on keeping the original protobuf around
    fn to_proto_bytes(&self) -> Result<Vec<u8>, String> {
        let signed_public_key_bundle_proto = self.signed_public_key_bundle_proto.clone();
        return signed_public_key_bundle_proto
            .write_to_bytes()
            .map_err(|e| e.to_string());
    }

    fn from_proto_bytes(bytes: &[u8]) -> Result<SignedPublicKeyBundle, String> {
        let signed_public_key_bundle_proto =
            proto::public_key::SignedPublicKeyBundle::parse_from_bytes(bytes);
        if signed_public_key_bundle_proto.is_err() {
            return Err(signed_public_key_bundle_proto.err().unwrap().to_string());
        }
        let signed_public_key_bundle = signed_public_key_bundle_proto.unwrap();
        // Check identity_key is populated
        if signed_public_key_bundle.identity_key.is_none() {
            return Err("No identity key found".to_string());
        }

        // Derive public key from SignedPublicKey
        let identity_key_result = public_key::signed_public_key_from_proto_v2(
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
        let pre_key_result = public_key::signed_public_key_from_proto_v2(
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

impl WalletAssociated for SignedPublicKeyBundle {
    fn wallet_address(&self) -> Result<String, String> {
        // Introspect proto and get the identity_key as a SignedPublicKey, then take the signature
        // and recover the wallet address
        let public_identity_key = self
            .signed_public_key_bundle_proto
            .identity_key
            .as_ref()
            .unwrap()
            .clone();

        let xmtp_sig_request_bytes =
            EthereumUtils::xmtp_identity_key_payload(public_identity_key.key_bytes.as_slice());
        let personal_sign_message =
            EthereumUtils::get_personal_sign_message(xmtp_sig_request_bytes.as_slice());
        let eth_address_result = public_key::recover_wallet_public_key(
            &personal_sign_message,
            &public_identity_key.signature,
        );
        if eth_address_result.is_err() {
            return Err(eth_address_result.err().unwrap().to_string());
        }

        let recovered_wallet_key = eth_address_result.unwrap();
        return Ok(recovered_wallet_key.get_ethereum_address());
    }
}

impl PartialEq for SignedPublicKeyBundle {
    fn eq(&self, other: &Self) -> bool {
        self.identity_key == other.identity_key && self.pre_key == other.pre_key
    }
}

impl VerifiablePublicKeyBundle<SignedPublicKey, SignedPublicKey> for SignedPublicKeyBundle {
    fn get_identity_key(&self) -> SignedPublicKey {
        self.identity_key.clone()
    }

    fn get_prekey(&self) -> SignedPublicKey {
        self.pre_key.clone()
    }

    fn verify_bundle_binding(&self) -> Result<(), String> {
        // Get the identity key and the pre key
        let identity_key = self.get_identity_key();
        let pre_key = self.get_prekey();

        // Get signed bytes from pre_key
        let pre_key_signed_bytes = pre_key.signed_bytes;

        // Parse the pre_key_signed_bytes into a PublicKey and check == pre_key
        let pre_key_from_signed_bytes =
            PublicKey::from_proto_bytes(pre_key_signed_bytes.as_slice())?;
        if pre_key_from_signed_bytes != pre_key.public_key {
            return Err("Pre key does not match signed bytes".to_string());
        }

        // Check the signature with the identity_key
        let identity_key_public_key = identity_key.public_key;

        let verify_result = identity_key_public_key.verify_sha256_signature(
            pre_key_signed_bytes.as_slice(),
            &pre_key.signature.signature_bytes,
        )?;
        if !verify_result {
            return Err("Pre key signature does not match identity key".to_string());
        }
        return Ok(());
    }
}
