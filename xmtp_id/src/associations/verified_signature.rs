#![allow(dead_code)]

use ed25519_dalek::{Signature as Ed25519Signature, VerifyingKey as Ed25519VerifyingKey};
use ethers::types::Signature as EthersSignature;
use ethers::utils::hash_message;
use ethers::{core::k256::ecdsa::VerifyingKey as EcdsaVerifyingKey, utils::public_key_to_address};
use sha2::{Digest, Sha512};
use xmtp_cryptography::signature::h160addr_to_string;
use xmtp_proto::xmtp::message_contents::SignedPublicKey as LegacySignedPublicKeyProto;

use crate::constants::INSTALLATION_KEY_SIGNATURE_CONTEXT;
use crate::scw_verifier::SmartContractSignatureVerifier;

use super::{
    AccountId, MemberIdentifier, SignatureError, SignatureKind, ValidatedLegacySignedPublicKey,
};

#[derive(Debug, Clone)]
pub struct VerifiedSignature {
    pub signer: MemberIdentifier,
    pub kind: SignatureKind,
    pub raw_bytes: Vec<u8>,
}

impl VerifiedSignature {
    pub fn new(signer: MemberIdentifier, kind: SignatureKind, raw_bytes: Vec<u8>) -> Self {
        Self {
            signer,
            kind,
            raw_bytes,
        }
    }

    /**
     * Verifies an ECDSA signature against the provided signature text.
     * Returns a VerifiedSignature if the signature is valid, otherwise returns an error.
     */
    pub fn from_recoverable_ecdsa<Text: AsRef<str>>(
        signature_text: Text,
        signature_bytes: &[u8],
    ) -> Result<Self, SignatureError> {
        let signature = EthersSignature::try_from(signature_bytes)?;
        let address = h160addr_to_string(signature.recover(signature_text.as_ref())?);

        Ok(Self::new(
            MemberIdentifier::Address(address),
            SignatureKind::Erc191,
            signature_bytes.to_vec(),
        ))
    }

    pub fn from_recoverable_ecdsa_with_expected_address<Text: AsRef<str>>(
        signature_text: Text,
        signature_bytes: &[u8],
        expected_address: Text,
    ) -> Result<Self, SignatureError> {
        let partially_verified = Self::from_recoverable_ecdsa(signature_text, signature_bytes)?;
        if partially_verified
            .signer
            .address()
            .ok_or(SignatureError::Invalid)?
            .to_lowercase()
            != expected_address.as_ref().to_lowercase()
        {
            return Err(SignatureError::Invalid);
        }

        Ok(partially_verified)
    }

    /**
     * Verifies an installation key signature against the provided signature text and verifying key bytes.
     * Returns a VerifiedSignature if the signature is valid, otherwise returns an error.
     */
    pub fn from_installation_key<Text: AsRef<str>>(
        signature_text: Text,
        signature_bytes: &[u8],
        verifying_key_bytes: &[u8],
    ) -> Result<Self, SignatureError> {
        let signature = Ed25519Signature::from_bytes(signature_bytes.try_into()?);
        let verifying_key = Ed25519VerifyingKey::from_bytes(verifying_key_bytes.try_into()?)?;
        let mut prehashed = Sha512::new();
        prehashed.update(signature_text.as_ref());

        verifying_key.verify_prehashed(
            prehashed,
            Some(INSTALLATION_KEY_SIGNATURE_CONTEXT),
            &signature,
        )?;

        Ok(Self::new(
            MemberIdentifier::Installation(verifying_key_bytes.to_vec()),
            SignatureKind::InstallationKey,
            signature_bytes.to_vec(),
        ))
    }

    pub fn from_legacy_delegated<Text: AsRef<str>>(
        signature_text: Text,
        signature_bytes: &[u8],
        signed_public_key_proto: LegacySignedPublicKeyProto,
    ) -> Result<Self, SignatureError> {
        let verified_legacy_signature =
            Self::from_recoverable_ecdsa(signature_text, signature_bytes)?;
        let signed_public_key: ValidatedLegacySignedPublicKey =
            signed_public_key_proto.try_into()?;
        let public_key = EcdsaVerifyingKey::from_sec1_bytes(&signed_public_key.public_key_bytes)?;
        let address = h160addr_to_string(public_key_to_address(&public_key));

        if MemberIdentifier::Address(address) != verified_legacy_signature.signer {
            return Err(SignatureError::Invalid);
        }

        Ok(Self::new(
            MemberIdentifier::Address(signed_public_key.account_address.to_lowercase()),
            SignatureKind::LegacyDelegated,
            // Must use the wallet signature bytes, since those are the ones we care about making unique.
            // This protects against using the legacy key more than once in the Identity Update Log
            signed_public_key.wallet_signature.raw_bytes,
        ))
    }

    pub async fn from_smart_contract_wallet<Text: AsRef<str>>(
        signature_text: Text,
        signature_verifier: &dyn SmartContractSignatureVerifier,
        signature_bytes: &[u8],
        account_id: AccountId,
        block_number: Option<u64>,
    ) -> Result<Self, SignatureError> {
        let is_valid = signature_verifier
            .is_valid_signature(
                account_id.clone(),
                hash_message(signature_text.as_ref()).into(),
                signature_bytes.to_vec().into(),
                block_number.map(|n| n.into()),
            )
            .await?;

        if is_valid {
            Ok(Self::new(
                MemberIdentifier::Address(account_id.into()),
                SignatureKind::Erc1271,
                signature_bytes.to_vec(),
            ))
        } else {
            Err(SignatureError::Invalid)
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::{
        associations::{
            sign_with_legacy_key, verified_signature::VerifiedSignature, MemberIdentifier,
            SignatureKind,
        },
        constants::INSTALLATION_KEY_SIGNATURE_CONTEXT,
        InboxOwner,
    };
    use ed25519_dalek::SigningKey as Ed25519SigningKey;
    use ethers::signers::{LocalWallet, Signer};
    use prost::Message;
    use xmtp_proto::xmtp::message_contents::{
        signature::Union as SignatureUnion, signed_private_key,
        SignedPrivateKey as LegacySignedPrivateKeyProto,
    };
    use xmtp_v2::k256_helper::sign_sha256;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_recoverable_ecdsa() {
        let wallet: LocalWallet = LocalWallet::new(&mut rand::thread_rng());
        let signature_text = "test signature body";

        let sig_bytes: Vec<u8> = wallet.sign_message(signature_text).await.unwrap().to_vec();
        let verified_sig = VerifiedSignature::from_recoverable_ecdsa(signature_text, &sig_bytes)
            .expect("should succeed");

        assert_eq!(verified_sig.signer.address().unwrap(), wallet.get_address());
        assert_eq!(verified_sig.kind, SignatureKind::Erc191);
        assert_eq!(verified_sig.raw_bytes, sig_bytes);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_recoverable_ecdsa_incorrect() {
        let wallet: LocalWallet = LocalWallet::new(&mut rand::thread_rng());
        let signature_text = "test signature body";

        let sig_bytes: Vec<u8> = wallet.sign_message(signature_text).await.unwrap().to_vec();

        let verified_sig =
            VerifiedSignature::from_recoverable_ecdsa("wrong text again", &sig_bytes).unwrap();
        assert_ne!(verified_sig.signer, wallet.get_address().into());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_installation_key() {
        let signing_key = Ed25519SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();

        let signature_text = "test signature text";
        let mut prehashed: Sha512 = Sha512::new();
        prehashed.update(signature_text);

        let sig = signing_key
            .sign_prehashed(prehashed, Some(INSTALLATION_KEY_SIGNATURE_CONTEXT))
            .unwrap();

        let verified_sig = VerifiedSignature::from_installation_key(
            signature_text,
            sig.to_bytes().as_slice(),
            verifying_key.as_bytes(),
        )
        .expect("should succeed");
        let expected = MemberIdentifier::Installation(verifying_key.as_bytes().to_vec());
        assert_eq!(expected, verified_sig.signer);
        assert_eq!(SignatureKind::InstallationKey, verified_sig.kind);
        assert_eq!(verified_sig.raw_bytes, sig.to_bytes().as_slice());

        // Make sure it fails with the wrong signature text
        VerifiedSignature::from_installation_key(
            "wrong signature text",
            sig.to_bytes().as_slice(),
            verifying_key.as_bytes(),
        )
        .expect_err("should fail with incorrect signature text");

        // Make sure it fails with the wrong verifying key
        VerifiedSignature::from_installation_key(
            signature_text,
            sig.to_bytes().as_slice(),
            &[verifying_key.as_bytes().as_slice(), vec![2, 3].as_slice()].concat(),
        )
        .expect_err("should fail with incorrect verifying key");
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn validate_good_key_round_trip() {
        let proto_bytes = vec![
            10, 79, 8, 192, 195, 165, 174, 203, 153, 231, 213, 23, 26, 67, 10, 65, 4, 216, 84, 174,
            252, 198, 225, 219, 168, 239, 166, 62, 233, 206, 108, 53, 155, 87, 132, 8, 43, 91, 36,
            91, 81, 93, 213, 67, 241, 69, 5, 31, 249, 186, 129, 119, 144, 4, 44, 54, 76, 185, 95,
            61, 23, 231, 72, 7, 169, 18, 70, 113, 79, 173, 82, 13, 37, 146, 201, 43, 174, 180, 33,
            125, 43, 18, 70, 18, 68, 10, 64, 7, 136, 100, 172, 155, 247, 230, 255, 253, 247, 78,
            50, 212, 226, 41, 78, 239, 183, 136, 247, 122, 88, 155, 245, 219, 183, 215, 202, 42,
            89, 162, 128, 96, 96, 120, 131, 17, 70, 38, 231, 2, 27, 91, 29, 66, 110, 128, 140, 1,
            42, 217, 185, 2, 181, 208, 100, 143, 143, 219, 159, 174, 1, 233, 191, 16, 1,
        ];
        let account_address = "0x220ca99fb7fafa18cb623d924794dde47b4bc2e9";

        let proto = LegacySignedPublicKeyProto::decode(proto_bytes.as_slice()).unwrap();
        let validated_key = ValidatedLegacySignedPublicKey::try_from(proto)
            .expect("Key should validate successfully");
        let proto: LegacySignedPublicKeyProto = validated_key.into();
        let validated_key = ValidatedLegacySignedPublicKey::try_from(proto)
            .expect("Key should still validate successfully");
        assert_eq!(validated_key.account_address(), account_address);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn validate_malformed_key() {
        let proto_bytes = vec![
            10, 79, 8, 192, 195, 165, 174, 203, 153, 231, 213, 23, 26, 67, 10, 65, 4, 216, 84, 174,
            252, 198, 225, 219, 168, 239, 166, 62, 233, 206, 108, 53, 155, 87, 132, 8, 43, 91, 36,
            91, 81, 93, 213, 67, 241, 69, 5, 31, 249, 186, 129, 119, 144, 4, 44, 54, 76, 185, 95,
            61, 23, 231, 72, 7, 169, 18, 70, 113, 79, 173, 82, 13, 37, 146, 201, 43, 174, 180, 33,
            125, 43, 18, 70, 18, 68, 10, 64, 7, 136, 100, 172, 155, 247, 230, 255, 253, 247, 78,
            50, 212, 226, 41, 78, 239, 183, 136, 247, 122, 88, 155, 245, 219, 183, 215, 202, 42,
            89, 162, 128, 96, 96, 120, 131, 17, 70, 38, 231, 2, 27, 91, 29, 66, 110, 128, 140, 1,
            42, 217, 185, 2, 181, 208, 100, 143, 143, 219, 159, 174, 1, 233, 191, 16, 1,
        ];
        let mut proto = LegacySignedPublicKeyProto::decode(proto_bytes.as_slice()).unwrap();
        proto.key_bytes[0] += 1; // Corrupt the serialized key data
        assert!(matches!(
            ValidatedLegacySignedPublicKey::try_from(proto),
            Err(super::SignatureError::Invalid)
        ));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_legacy_delegated() {
        let signature_text = "test_legacy_signature";
        let account_address = "0x0bd00b21af9a2d538103c3aaf95cb507f8af1b28".to_string();
        let legacy_signed_private_key = hex::decode("0880bdb7a8b3f6ede81712220a20ad528ea38ce005268c4fb13832cfed13c2b2219a378e9099e48a38a30d66ef991a96010a4c08aaa8e6f5f9311a430a41047fd90688ca39237c2899281cdf2756f9648f93767f91c0e0f74aed7e3d3a8425e9eaa9fa161341c64aa1c782d004ff37ffedc887549ead4a40f18d1179df9dff124612440a403c2cb2338fb98bfe5f6850af11f6a7e97a04350fc9d37877060f8d18e8f66de31c77b3504c93cf6a47017ea700a48625c4159e3f7e75b52ff4ea23bc13db77371001").unwrap();

        // happy path
        let legacy_signature = sign_with_legacy_key(
            signature_text.to_string(),
            legacy_signed_private_key.clone(),
        )
        .await
        .unwrap();
        let expected = MemberIdentifier::Address(account_address.clone());
        let verified_sig = VerifiedSignature::from_legacy_delegated(
            signature_text,
            &legacy_signature.legacy_key_signature.signature_bytes,
            legacy_signature.signed_public_key_proto.clone(),
        )
        .expect("should succeed");

        let legacy_signature_bytes = match legacy_signature
            .signed_public_key_proto
            .signature
            .unwrap()
            .union
            .unwrap()
        {
            SignatureUnion::WalletEcdsaCompact(legacy_wallet_ecdsa) => [
                legacy_wallet_ecdsa.bytes,
                vec![legacy_wallet_ecdsa.recovery as u8],
            ]
            .concat(),
            _ => panic!("Invalid signature type"),
        };

        assert_eq!(verified_sig.signer, expected);
        assert_eq!(verified_sig.kind, SignatureKind::LegacyDelegated);
        assert_eq!(verified_sig.raw_bytes, legacy_signature_bytes,);

        // fail path
        let legacy_signed_private_key_proto =
            LegacySignedPrivateKeyProto::decode(legacy_signed_private_key.as_slice()).unwrap();
        let signed_private_key::Union::Secp256k1(secp256k1) =
            legacy_signed_private_key_proto.union.unwrap();
        let legacy_private_key = secp256k1.bytes;
        let (mut legacy_signature, recovery_id) = sign_sha256(
            &legacy_private_key,       // secret_key
            signature_text.as_bytes(), // message
        )
        .unwrap();
        legacy_signature.push(recovery_id);
        let legacy_signed_public_key_proto = legacy_signed_private_key_proto.public_key.unwrap();

        let res = VerifiedSignature::from_legacy_delegated(
            signature_text,
            &legacy_signature,
            legacy_signed_public_key_proto,
        );
        assert!(matches!(res, Err(super::SignatureError::Invalid)));
    }
}
