use ethers::core::rand::thread_rng;
use ethers::signers::{LocalWallet, Signer, coins_bip39::{Mnemonic,English}};

// base64 decoding library
use base64::decode;
use hex::encode;

mod proto;

pub struct XMTP {}

impl XMTP {
    pub fn generate_mnemonic(&self) -> String {
		let mut rng = rand::thread_rng();
		let mnemonic = Mnemonic::<English>::new_with_count(&mut rng, 12).unwrap();
		let phrase = mnemonic.to_phrase();
		// split the phrase by spaces
		let words: Vec<String> = phrase.unwrap().split(" ").map(|s| s.to_string()).collect();
        return words.join(" ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_mnemonic_works() {
        let x = XMTP {};
        let mnemonic = x.generate_mnemonic();
        assert_eq!(mnemonic.split(" ").count(), 12);
    }
}
