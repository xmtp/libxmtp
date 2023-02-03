use ethers::core::rand::thread_rng;
use ethers::signers::{coins_bip39::{Mnemonic,English}};

mod proto;

pub struct Keystore {}

impl Keystore {
    // new() is a constructor for the Keystore struct
    pub fn new() -> Self {
        Keystore {}
    }

    pub fn generate_mnemonic(&self) -> String {
		let mut rng = thread_rng();
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
