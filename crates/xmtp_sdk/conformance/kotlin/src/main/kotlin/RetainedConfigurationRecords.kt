import com.sun.jna.Native
import uniffi.xmtp_sdk.*

internal fun checkNativeConfigurationRecordProjection() {
    val pointerBits = Native.POINTER_SIZE * 8
    System.getenv("SDK_CONFORMANCE_POINTER_BITS")?.let {
        check(it.toInt() == pointerBits) { "the recorded ABI does not match the loaded JVM" }
    }
    checkConfigurationRecordProjection(::sdkConformanceServerConfigurationSample, pointerBits)
}

// The shared writer supplies this private native fixture. Never construct the
// input record in Kotlin: this check must observe native record lifting.
internal fun checkConfigurationRecordProjection(
    sample: (Boolean?) -> ServerConfiguration,
    expectedPointerBits: Int,
) {
    val envelopeMaximum =
        when (expectedPointerBits) {
            32 -> UInt.MAX_VALUE.toULong()
            64 -> ULong.MAX_VALUE
            else -> error("unsupported ABI pointer width: $expectedPointerBits")
        }
    for (flag in listOf(null, false, true)) {
        val value = sample(flag)
        check(value.identifier == "kotlin-configuration-probe")
        check(value.serverVersion == "2.3.4" && value.minLibxmtpVersion == "1.2.3")
        check(value.auth.enabled)
        check(value.auth.keys == listOf(SigningKeyDescription("probe-key", "ES256")))
        check(value.auth.audiences == listOf("probe-audience"))
        check(value.auth.issuers == listOf("probe-issuer"))
        check(value.auth.requiredScopes == listOf("message:read", "message:write"))
        check(value.retention.groupMessageSeconds == 31uL)
        check(value.retention.welcomeSeconds == 32uL && value.retention.keyPackageSeconds == 33uL)
        check(value.mls.maxGroupMembers == 21uL && value.mls.maxInstallationsPerInbox == 22uL)
        check(value.mls.commitLogEnabled == flag) { "native nullable commit-log flag changed" }
        check(value.smartContractWalletChains == listOf("eip155:1", "eip155:31337"))
        val limits = value.limits
        check(limits.maxEnvelopeBytes == envelopeMaximum) { "native usize maximum changed during record lifting" }
        check(limits.maxRequestBytes == 2uL && limits.maxResponseBytes == 3uL)
        check(limits.maxPublishTopics == 4uL && limits.maxQueryTopics == 5uL)
        check(limits.maxQueryLimit == 6uL && limits.defaultQueryLimit == 7uL)
        check(limits.maxNewestMetadataTopics == 8uL && limits.maxNewestFullTopics == 9uL)
        check(limits.maxUpdateAdds == 10uL && limits.maxUpdateRemoves == 11uL)
        check(limits.maxStreamTopics == 12uL && limits.maxStaticTopics == 13uL)
        check(limits.maxLookupIdentifiers == 14uL && limits.maxScwSignatures == 15uL)
        check(limits.maxIdentityEntries == 16uL)
        check(limits.maxUpdateFramesPerSecond == 2_147_483_648u)
        check(limits.maxUpdateBurst == 18u && limits.maxPingFramesPerSecond == 19u && limits.maxPingBurst == 20u)
        val attachments = checkNotNull(value.attachments)
        check(attachments.baseUrl == "https://files.example/v1/")
        check(attachments.maxUploadBytes == 9_007_199_254_741_025uL && attachments.retentionSeconds == 41uL)
        val policy = MetadataPolicy.Base(MetadataBasePolicy.Allow)
        check(
            value.applicationComponents ==
                listOf(
                    ApplicationComponentDefinition(
                        0xC321u,
                        "configuration_host_probe",
                        MetadataComponentType.String,
                        ComponentPermissions(policy, policy, policy),
                        inGroups = true,
                        inDms = false,
                    ),
                ),
        )
    }
    println("Kotlin retained native configuration projection: ABI ${expectedPointerBits}bit, maximum $envelopeMaximum")
    println("Configuration fields, null, false and true u64 upload ceiling passed")
}
