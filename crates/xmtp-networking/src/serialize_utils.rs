use base64::Engine;
use serde::Serializer;

pub fn as_base64<S>(
    data: &[u8],
    serializer: S,
) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(data))
}
