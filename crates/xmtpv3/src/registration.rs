use base64::{engine::general_purpose, Engine as b64Engine};
use ethers::types::Signature as EthSignature;
use iri_string::types::UriString;
use siwe::Message as SiweMessage;

extern crate alloc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct XmtpError {
    reason: String,
}

impl From<String> for XmtpError {
    fn from(error: String) -> Self {
        XError { reason: error }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySignature {
    SIWE {
        text: SiweMessage,
        sig: EthSignature,
    }
}

#[allow(dead_code)]
impl KeySignature {
    fn verify(&self) -> bool {
        match self {
            KeySignature::SIWE { text, sig } => sig.verify(text.to_string(), text.address).is_ok(),
        }
    }

    fn from_siwe(text: SiweMessage, sig: EthSignature) -> Result<Self, String> {
        let ks = KeySignature::SIWE {
            text: text,
            sig: sig,
        };

        if ks.verify() {
            Ok(ks)
        } else {
            Err("SigVal Failed".to_string())
        }
    }
}

#[allow(dead_code)]
pub struct Registration {
    key_bytes: [u8; 32],
}

#[allow(dead_code)]
impl Registration {
    pub fn new() -> Self {
        // TODO: Integrate with real account info
        Registration { key_bytes: [0; 32] }
    }

    /// Add XMTP specific resources to an existing SIWE text
    pub fn build_siwe(&self, mut siwe: SiweMessage) -> SiweMessage {
        let key_string = general_purpose::STANDARD.encode(self.key_bytes);

        siwe.resources.push(
            format!("xmtp:Create-New-Device-identity:{}", key_string)
                .parse::<UriString>()
                .unwrap(),
        );
        siwe
    }

    pub fn finalize(
        &self,
        text: SiweMessage,
        sig: EthSignature,
        func_publish: fn(KeySignature) -> Result<bool, String>,
    ) -> Result<bool, XmtpError> {
        let skey = KeySignature::from_siwe(text, sig).map_err(XmtpError::from)?;
        let b_ok = func_publish(skey)?;

        if b_ok {
            Ok(b_ok)
        } else {
            Err(XmtpError {
                reason: String::from("Publish did not succeed"),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use ethers::{
        signers::{LocalWallet, Signer},
        types::{ Signature as EthSignature},
    };

    use eip55::checksum;
    use rand_chacha::rand_core::SeedableRng;
    use siwe::{generate_nonce, Message as SiweMessage};
    use super::{KeySignature, Registration};
    use futures::executor::block_on;

    pub fn new_wallet() -> LocalWallet {
        LocalWallet::new(&mut rand_chacha::ChaCha20Rng::seed_from_u64(123))
    }

    fn generate_siwe(addr: &String) -> SiweMessage {
        let nonce = generate_nonce();
        let m: SiweMessage = format!(
            "service.invalid wants you to sign in with your Ethereum account:
{addr}

I accept the ServiceOrg Terms of Service: https://service.invalid/tos

URI: https://service.invalid/login
Version: 1
Chain ID: 1
Nonce: {nonce}
Issued At: 2021-09-30T16:25:24Z
Resources:
- ipfs://bafybeiemxf5abjwjbikoz4mc3a3dla6ual3jsgpdr4cjr3oz3evfyavhwq/
- https://example.com/my-web2-claim.json"
        )
        .parse()
        .unwrap();
        m
    }

    fn publish(k: KeySignature) -> Result<bool, String> {
        println!("Publish: {:?}", k);
        return Ok(true);
    }

    /// Ethers-rs doesn't support a mechanism to get the Address String from an H160.
    /// This function stringifys the hash and then converts to eip55 format.
    fn get_addr(w: &LocalWallet) -> String {
        let addr = format!("{:x}", w.address());
        checksum(&addr)
    }

    #[test]
    fn app_reg_devtest() {
        let w = new_wallet();

        // Simulate an App
        let app_siwe = generate_siwe(&get_addr(&w));
        println!(
            "=========== This is the Apps SIwE:\n{}\n========\n",
            app_siwe
        );

        let r = Registration::new();
        let final_app_siwe = r.build_siwe(app_siwe);

        println!(
            "=========== This is the final SIwE:\n{}\n========\n",
            final_app_siwe
        );

        // Sign Message -- Ethers uses Futures so we block to get the value in this sync context.
        let signature: EthSignature =
            block_on(w.sign_message(&final_app_siwe.to_string())).unwrap();

        r.finalize(final_app_siwe, signature, publish).unwrap();
    }

    #[test]
    fn app_reg2() {
        let w = new_wallet();
        let m = new_wallet();

        let msg = "MESSAGE";

        // Sign Message -- Ethers uses Futures so we block to get the value in this sync context.
        let signature: EthSignature = block_on(w.sign_message(&msg)).unwrap();

        signature.verify(msg, w.address())?
    }
}
