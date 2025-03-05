#![allow(dead_code)]
use ethers::types::Signature as EthersSignature;
use ethers::utils::hash_message;
use ethers::{core::k256::ecdsa::VerifyingKey as EcdsaVerifyingKey, utils::public_key_to_address};
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use p256::EncodedPoint;
use sha2::{Digest, Sha256};
use xmtp_cryptography::signature::h160addr_to_string;
use xmtp_cryptography::CredentialVerify;
use xmtp_proto::xmtp::message_contents::SignedPublicKey as LegacySignedPublicKeyProto;

use crate::scw_verifier::SmartContractSignatureVerifier;

use super::{
    ident, to_lower_s, AccountId, InstallationKeyContext, MemberIdentifier, SignatureError,
    SignatureKind, ValidatedLegacySignedPublicKey,
};

#[derive(Debug, Clone)]
pub struct VerifiedSignature {
    pub signer: MemberIdentifier,
    pub kind: SignatureKind,
    pub raw_bytes: Vec<u8>,
    pub chain_id: Option<u64>,
}

impl VerifiedSignature {
    pub fn new(
        signer: MemberIdentifier,
        kind: SignatureKind,
        raw_bytes: Vec<u8>,
        chain_id: Option<u64>,
    ) -> Self {
        Self {
            signer,
            kind,
            raw_bytes,
            chain_id,
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
        let normalized_signature_bytes = to_lower_s(signature_bytes)?;
        let signature = EthersSignature::try_from(normalized_signature_bytes.as_slice())?;
        let address = h160addr_to_string(signature.recover(signature_text.as_ref())?);

        Ok(Self::new(
            MemberIdentifier::eth(address)?,
            SignatureKind::Erc191,
            normalized_signature_bytes.to_vec(),
            None,
        ))
    }

    /**
     * Verifies an ECDSA signature against the provided signature text and ensures that the recovered
     * address matches the expected address.
     */
    pub fn from_recoverable_ecdsa_with_expected_address<Text: AsRef<str>>(
        signature_text: Text,
        signature_bytes: &[u8],
        expected_address: Text,
    ) -> Result<Self, SignatureError> {
        let partially_verified = Self::from_recoverable_ecdsa(signature_text, signature_bytes)?;
        if partially_verified
            .signer
            .eth_address()
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
        verifying_key: ed25519_dalek::VerifyingKey,
    ) -> Result<Self, SignatureError> {
        verifying_key.credential_verify::<InstallationKeyContext>(
            signature_text,
            signature_bytes.try_into()?,
        )?;
        Ok(Self::new(
            MemberIdentifier::installation(verifying_key.as_bytes().to_vec()),
            SignatureKind::InstallationKey,
            signature_bytes.to_vec(),
            None,
        ))
    }

    pub fn from_passkey(
        client_data_json: &str,
        authenticator_data: &[u8],
        signature_bytes: &[u8],
        public_key: &[u8],
        relying_partner: Option<String>,
    ) -> Result<Self, SignatureError> {
        let encoded_point = EncodedPoint::from_bytes(public_key).unwrap();
        let verifying_key = VerifyingKey::from_encoded_point(&encoded_point).unwrap();

        let mut client_data_hash = Sha256::new();
        client_data_hash.update(client_data_json.as_bytes());
        let client_data_hash = client_data_hash.finalize();

        let mut message = Vec::with_capacity(authenticator_data.len() + client_data_hash.len());
        message.extend_from_slice(authenticator_data);
        message.extend_from_slice(&client_data_hash);

        let mut message_hash = Sha256::new();
        message_hash.update(&message);
        let message_hash = message_hash.finalize();

        let signature = Signature::try_from(signature_bytes).unwrap();

        verifying_key.verify(&message_hash, &signature).unwrap();

        Ok(Self::new(
            MemberIdentifier::Passkey(ident::Passkey {
                key: public_key.to_vec(),
                relying_partner,
            }),
            SignatureKind::P256,
            signature_bytes.to_vec(),
            None,
        ))
    }

    /// Verifies a legacy delegated signature and recovers the wallet address responsible
    /// associated with the signer.
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

        if MemberIdentifier::eth(address)? != verified_legacy_signature.signer {
            return Err(SignatureError::Invalid);
        }

        Ok(Self::new(
            MemberIdentifier::eth(signed_public_key.account_address)?,
            SignatureKind::LegacyDelegated,
            // Must use the wallet signature bytes, since those are the ones we care about making unique.
            // This protects against using the legacy key more than once in the Identity Update Log
            signed_public_key.wallet_signature.raw_bytes,
            None,
        ))
    }

    /// Verifies a smart contract wallet signature using the provided signature verifier.
    pub async fn from_smart_contract_wallet<Text: AsRef<str>>(
        signature_text: Text,
        signature_verifier: impl SmartContractSignatureVerifier,
        signature_bytes: &[u8],
        account_id: AccountId,
        block_number: &mut Option<u64>,
    ) -> Result<Self, SignatureError> {
        let response = signature_verifier
            .is_valid_signature(
                account_id.clone(),
                hash_message(signature_text.as_ref()).into(),
                signature_bytes.to_vec().into(),
                block_number.map(|n| n.into()),
            )
            .await?;

        if response.is_valid {
            // set the block the signature was validated on
            *block_number = response.block_number;

            Ok(Self::new(
                MemberIdentifier::eth(account_id.get_account_address())?,
                SignatureKind::Erc1271,
                signature_bytes.to_vec(),
                Some(account_id.get_chain_id_u64()?),
            ))
        } else {
            tracing::error!(
                "Smart contract wallet signature is invalid {:?}",
                response.error
            );
            Err(SignatureError::Invalid)
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::associations::{
        test_utils::{MockSmartContractSignatureVerifier, WalletTestExt},
        verified_signature::VerifiedSignature,
        InstallationKeyContext, MemberIdentifier, SignatureKind,
    };
    use ethers::signers::{LocalWallet, Signer};
    use prost::Message;
    use xmtp_common::rand_hexstring;
    use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_recoverable_ecdsa() {
        let wallet: LocalWallet = LocalWallet::new(&mut rand::thread_rng());
        let signature_text = "test signature body";

        let sig_bytes: Vec<u8> = wallet.sign_message(signature_text).await.unwrap().to_vec();
        let verified_sig = VerifiedSignature::from_recoverable_ecdsa(signature_text, &sig_bytes)
            .expect("should succeed");

        assert_eq!(verified_sig.signer, wallet.member_identifier());
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
        assert_ne!(verified_sig.signer, wallet.member_identifier());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_installation_key() {
        let key = XmtpInstallationCredential::new();
        let verifying_key = key.verifying_key();
        let signature_text = "test signature text";
        let sig = key
            .credential_sign::<InstallationKeyContext>(signature_text)
            .unwrap();

        let verified_sig =
            VerifiedSignature::from_installation_key(signature_text, sig.as_slice(), verifying_key)
                .expect("should succeed");
        let expected = MemberIdentifier::installation(verifying_key.as_bytes().to_vec());
        assert_eq!(expected, verified_sig.signer);
        assert_eq!(SignatureKind::InstallationKey, verified_sig.kind);
        assert_eq!(verified_sig.raw_bytes, sig.as_slice());

        // Make sure it fails with the wrong signature text
        VerifiedSignature::from_installation_key(
            "wrong signature text",
            sig.as_slice(),
            verifying_key,
        )
        .expect_err("should fail with incorrect signature text");

        // Make sure it fails with the wrong verifying key
        VerifiedSignature::from_installation_key(
            signature_text,
            sig.as_slice(),
            XmtpInstallationCredential::new().verifying_key(),
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
    async fn test_smart_contract_wallet() {
        let mock_verifier = MockSmartContractSignatureVerifier::new(true);
        let chain_id: u64 = 24;
        let account_address = rand_hexstring();
        let account_id = AccountId::new(format!("eip155:{chain_id}"), account_address.clone());
        let signature_text = "test_smart_contract_wallet_signature";
        let signature_bytes = &[1, 2, 3];
        let mut block_number = Some(1);

        let verified_sig = VerifiedSignature::from_smart_contract_wallet(
            signature_text,
            mock_verifier,
            signature_bytes,
            account_id,
            &mut block_number,
        )
        .await
        .expect("should validate");
        assert_eq!(
            verified_sig.signer,
            MemberIdentifier::eth(account_address).unwrap()
        );
        assert_eq!(verified_sig.kind, SignatureKind::Erc1271);
        assert_eq!(verified_sig.raw_bytes, signature_bytes);
        assert_eq!(verified_sig.chain_id, Some(chain_id));
    }
}
