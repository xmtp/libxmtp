use super::hash::sha256;

pub fn serialize_group_id(group_id: &[u8]) -> String {
    hex::encode(group_id)
}

/// Relies on a client-created idempotency_key (which could be a timestamp)
pub fn calculate_message_id(
    group_id: &[u8],
    decrypted_message_bytes: &[u8],
    idempotency_key: &str,
) -> Vec<u8> {
    let separator = b"\t";
    let mut id_vec = Vec::new();
    id_vec.extend_from_slice(group_id);
    id_vec.extend_from_slice(separator);
    id_vec.extend_from_slice(idempotency_key.as_bytes());
    id_vec.extend_from_slice(separator);
    id_vec.extend_from_slice(decrypted_message_bytes);
    sha256(&id_vec)
}
