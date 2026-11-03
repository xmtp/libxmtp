use hpke_rs::{
    Hpke, Mode,
    hpke_types::{AeadAlgorithm, KdfAlgorithm, KemAlgorithm},
    libcrux::HpkeLibcrux,
};
fn main() {
    #[allow(deprecated)]
    let mut hpke = Hpke::<HpkeLibcrux>::new(
        Mode::Base,
        KemAlgorithm::XWingDraft06Obsolete,
        KdfAlgorithm::HkdfSha256,
        AeadAlgorithm::ChaCha20Poly1305,
    );
    let ikm = [7u8; 32];
    let pair = hpke.derive_key_pair(&ikm).unwrap();
    let label = b"MLS 1.0 MLS_WELCOME";
    let mut info = vec![label.len() as u8];
    info.extend_from_slice(label);
    info.push(0);
    let (enc, mut sender) = hpke
        .setup_sender(pair.public_key(), &info, None, None, None)
        .unwrap();
    let payload = b"persisted Welcome compatibility fixture";
    let secondary = b"persisted Welcome metadata";
    let ct = sender.seal(&[], payload).unwrap();
    let ct2 = sender.seal(&[], secondary).unwrap();
    let mut receiver = hpke
        .setup_receiver(&enc, pair.private_key(), &info, None, None, None)
        .unwrap();
    assert_eq!(receiver.open(&[], &ct).unwrap(), payload);
    assert_eq!(receiver.open(&[], &ct2).unwrap(), secondary);
    let fixture = serde_json::json!({"ikm":hex::encode(ikm),"public_key":hex::encode(pair.public_key().as_slice()),"private_key":hex::encode(pair.private_key().as_slice()),"kem_output":hex::encode(enc),"ciphertext":hex::encode(ct),"secondary_ciphertext":hex::encode(ct2),"payload":hex::encode(payload),"secondary_payload":hex::encode(secondary)});
    let args: Vec<_> = std::env::args().collect();
    if let Some(path) = args.get(1) {
        let other: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(fixture, other, "old and new bytes must match");
        let enc = hex::decode(other["kem_output"].as_str().unwrap()).unwrap();
        let ct = hex::decode(other["ciphertext"].as_str().unwrap()).unwrap();
        let ct2 = hex::decode(other["secondary_ciphertext"].as_str().unwrap()).unwrap();
        let sk = hpke_rs::HpkePrivateKey::new(
            hex::decode(other["private_key"].as_str().unwrap()).unwrap(),
        );
        let mut receiver = hpke
            .setup_receiver(&enc, &sk, &info, None, None, None)
            .unwrap();
        assert_eq!(receiver.open(&[], &ct).unwrap(), payload);
        assert_eq!(receiver.open(&[], &ct2).unwrap(), secondary);
    }
    println!("{}", serde_json::to_string_pretty(&fixture).unwrap());
}
