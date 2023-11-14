use super::hash::sha256;

pub fn serialize_group_id(group_id: &[u8]) -> String {
    // TODO: I wonder if we really want to be base64 encoding this or if we can treat it as a
    // slice
    hex::encode(group_id)
}

pub fn get_message_id(
    decrypted_message_bytes: &[u8],
    group_id: &[u8],
    envelope_timestamp_ns: u64,
) -> Vec<u8> {
    let mut id_vec = Vec::new();
    id_vec.extend_from_slice(group_id);
    id_vec.extend_from_slice(&envelope_timestamp_ns.to_be_bytes());
    id_vec.extend_from_slice(decrypted_message_bytes);
    sha256(&id_vec)
}
