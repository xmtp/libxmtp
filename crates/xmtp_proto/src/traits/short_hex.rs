use crate::types::InstallationId;

const SHORT_LEN: usize = 4;

pub trait ShortHex {
    fn short_hex(&self) -> String;
}

impl ShortHex for &[u8] {
    fn short_hex(&self) -> String {
        short_hex(self)
    }
}
impl ShortHex for InstallationId {
    fn short_hex(&self) -> String {
        self.as_slice().short_hex()
    }
}
impl ShortHex for Vec<u8> {
    fn short_hex(&self) -> String {
        self.as_slice().short_hex()
    }
}

fn short_hex(bytes: &[u8]) -> String {
    let len = SHORT_LEN.min(bytes.len());
    hex::encode(&bytes[..len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_hex() {
        let hex = "5bf078bd83995fe83092d93c5655f059";
        let bytes = hex::decode(hex).unwrap();
        let short_hex = short_hex(&bytes);

        assert_eq!(short_hex.len(), SHORT_LEN * 2);
        assert_eq!(hex[..short_hex.len()], short_hex);
    }
}
