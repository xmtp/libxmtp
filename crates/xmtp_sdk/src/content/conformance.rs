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
