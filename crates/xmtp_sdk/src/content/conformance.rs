/// Native host conformance compares each host codec with the core encoder.
#[cfg(feature = "conformance")]
#[derive(Clone, Debug, uniffi::Record)]
pub struct StandardCodecSample {
    pub value: StandardContent,
    pub expected: EncodedContent,
}

#[cfg(feature = "conformance")]
#[xmtp_macro::sdk_export(pure)]
pub fn sdk_conformance_standard_samples() -> Vec<StandardCodecSample> {
    pure_codec_tests::standard_codec_samples()
        .expect("fixed standard codec samples")
        .into_iter()
        .map(|(value, expected)| StandardCodecSample {
            value,
            expected: expected.try_into().expect("fixed standard codec envelope"),
        })
        .collect()
}

// These exports are absent from the default SDK and the pure codec module.
#[cfg(all(feature = "conformance", not(feature = "pure-only")))]
#[xmtp_macro::sdk_export]
pub fn sdk_conformance_watch_text_decode(text: String) {
    xmtp_mls::messages::decoded_message::decode_counter::watch(text);
}

#[cfg(all(feature = "conformance", not(feature = "pure-only")))]
#[xmtp_macro::sdk_export]
pub fn sdk_conformance_text_decode_count() -> u64 {
    xmtp_mls::messages::decoded_message::decode_counter::count()
}
