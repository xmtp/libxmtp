use hkdf::Hkdf;
use sha3::Sha3_256;

pub fn hkdf(secret: &[u8], salt: &[u8]) -> Result<[u8; 32], String> {
    let hk = Hkdf::<Sha3_256>::new(Some(salt), secret);
    let mut okm = [0u8; 42];
    let res = hk.expand(&[], &mut okm);
    if res.is_err() {
        return Err(res.err().unwrap().to_string());
    }
    okm[0..32]
        .try_into()
        .map_err(|_| "hkdf failed to fit in 32 bytes".to_string())
}
