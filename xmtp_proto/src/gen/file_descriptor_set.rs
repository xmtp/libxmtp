pub const IDENTITY_API_V1_DESCRIPTOR_SET: &'static [u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/identity_api_v1.bin"));

pub const MLS_API_V1_DESCRIPTOR_SET: &'static [u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/mls_api_v1.bin"));

pub static MESSAGE_API_DESCRIPTOR_SET: &'static [u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/message_api.bin"));

pub static PAYER_API_DESCRIPTOR_SET: &'static [u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/payer_api.bin"));
