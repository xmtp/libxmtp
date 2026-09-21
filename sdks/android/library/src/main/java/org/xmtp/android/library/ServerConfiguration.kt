package org.xmtp.android.library

import uniffi.xmtpv3.FfiAuthConfiguration
import uniffi.xmtpv3.FfiException
import uniffi.xmtpv3.FfiLimitsConfiguration
import uniffi.xmtpv3.FfiMlsConfiguration
import uniffi.xmtpv3.FfiRetentionConfiguration
import uniffi.xmtpv3.FfiServerConfiguration
import uniffi.xmtpv3.FfiSigningKeyDescription

// The six server-configuration conditions.
//
// libxmtp raises each one as its own `FfiException` subclass, and this SDK lets
// native errors reach the app rather than re-wrapping them, the way it already
// does for every other client call. These aliases are therefore the public
// contract: catch them by type, never by matching a message. Each carries only
// the error text, because the underlying error is flat across the FFI boundary;
// the values behind a condition — the accepted chains, the required scopes, the
// limits — are read from `ServerConfiguration` instead.

/** The deployment did not serve its configuration, or it could not be stored. */
typealias ConfigurationUnavailableException = FfiException.ConfigurationUnavailable

/** The deployment published a configuration this client cannot use. */
typealias ConfigurationInvalidException = FfiException.ConfigurationInvalid

/** This database is bound to one deployment and a different one answered. */
typealias BackendMismatchException = FfiException.BackendMismatch

/** The deployment requires a newer libxmtp than this build. */
typealias ClientVersionTooOldException = FfiException.ClientVersionTooOld

/** The deployment requires a credential and none was configured. */
typealias AuthRequiredException = FfiException.AuthRequired

/** The deployment does not verify wallet signatures on that chain. */
typealias ChainNotAcceptedException = FfiException.ChainNotAccepted

/** Public identity of one accepted signing key. Never the key itself. */
data class SigningKeyDescription(
    val kid: String,
    val alg: String,
) {
    internal companion object {
        internal fun fromFfi(key: FfiSigningKeyDescription): SigningKeyDescription =
            SigningKeyDescription(kid = key.kid, alg = key.alg)
    }
}

/**
 * What a client must present to be admitted. An app acts on [enabled] and
 * [requiredScopes]; the rest is published for operator tooling.
 */
data class AuthConfiguration(
    val enabled: Boolean,
    val keys: List<SigningKeyDescription>,
    val audiences: List<String>,
    val issuers: List<String>,
    val requiredScopes: List<String>,
) {
    internal companion object {
        internal fun fromFfi(auth: FfiAuthConfiguration): AuthConfiguration =
            AuthConfiguration(
                enabled = auth.enabled,
                keys = auth.keys.map { SigningKeyDescription.fromFfi(it) },
                audiences = auth.audiences,
                issuers = auth.issuers,
                requiredScopes = auth.requiredScopes,
            )
    }
}

/** How long the deployment keeps each payload kind, in seconds. */
data class RetentionConfiguration(
    val groupMessageSeconds: Long,
    val welcomeSeconds: Long,
    val keyPackageSeconds: Long,
) {
    internal companion object {
        internal fun fromFfi(retention: FfiRetentionConfiguration): RetentionConfiguration =
            RetentionConfiguration(
                groupMessageSeconds = retention.groupMessageSeconds.published("retention.groupMessageSeconds"),
                welcomeSeconds = retention.welcomeSeconds.published("retention.welcomeSeconds"),
                keyPackageSeconds = retention.keyPackageSeconds.published("retention.keyPackageSeconds"),
            )
    }
}

/**
 * Request shapes the deployment accepts. The client chunks its own work to these
 * values; an app reads them to size its batches.
 */
data class LimitsConfiguration(
    val maxEnvelopeBytes: Long,
    val maxRequestBytes: Long,
    val maxResponseBytes: Long,
    val maxPublishTopics: Long,
    val maxQueryTopics: Long,
    val maxQueryLimit: Long,
    val defaultQueryLimit: Long,
    val maxNewestMetadataTopics: Long,
    val maxNewestFullTopics: Long,
    val maxUpdateAdds: Long,
    val maxUpdateRemoves: Long,
    val maxStreamTopics: Long,
    val maxStaticTopics: Long,
    val maxLookupIdentifiers: Long,
    val maxScwSignatures: Long,
    val maxIdentityEntries: Long,
    /**
     * The four rates the wire publishes as `uint32` stay unsigned, because a
     * deployment may publish any value a `uint32` holds and a signed [Int]
     * would have to reject the top half of that range.
     */
    val maxUpdateFramesPerSecond: UInt,
    val maxUpdateBurst: UInt,
    val maxPingFramesPerSecond: UInt,
    val maxPingBurst: UInt,
) {
    internal companion object {
        internal fun fromFfi(limits: FfiLimitsConfiguration): LimitsConfiguration =
            LimitsConfiguration(
                maxEnvelopeBytes = limits.maxEnvelopeBytes.published("limits.maxEnvelopeBytes"),
                maxRequestBytes = limits.maxRequestBytes.published("limits.maxRequestBytes"),
                maxResponseBytes = limits.maxResponseBytes.published("limits.maxResponseBytes"),
                maxPublishTopics = limits.maxPublishTopics.published("limits.maxPublishTopics"),
                maxQueryTopics = limits.maxQueryTopics.published("limits.maxQueryTopics"),
                maxQueryLimit = limits.maxQueryLimit.published("limits.maxQueryLimit"),
                defaultQueryLimit = limits.defaultQueryLimit.published("limits.defaultQueryLimit"),
                maxNewestMetadataTopics = limits.maxNewestMetadataTopics.published("limits.maxNewestMetadataTopics"),
                maxNewestFullTopics = limits.maxNewestFullTopics.published("limits.maxNewestFullTopics"),
                maxUpdateAdds = limits.maxUpdateAdds.published("limits.maxUpdateAdds"),
                maxUpdateRemoves = limits.maxUpdateRemoves.published("limits.maxUpdateRemoves"),
                maxStreamTopics = limits.maxStreamTopics.published("limits.maxStreamTopics"),
                maxStaticTopics = limits.maxStaticTopics.published("limits.maxStaticTopics"),
                maxLookupIdentifiers = limits.maxLookupIdentifiers.published("limits.maxLookupIdentifiers"),
                maxScwSignatures = limits.maxScwSignatures.published("limits.maxScwSignatures"),
                maxIdentityEntries = limits.maxIdentityEntries.published("limits.maxIdentityEntries"),
                maxUpdateFramesPerSecond = limits.maxUpdateFramesPerSecond,
                maxUpdateBurst = limits.maxUpdateBurst,
                maxPingFramesPerSecond = limits.maxPingFramesPerSecond,
                maxPingBurst = limits.maxPingBurst,
            )
    }
}

/**
 * Advisory group shapes. The deployment publishes them; the client enforces them.
 */
data class MlsConfiguration(
    val maxGroupMembers: Long,
    val maxInstallationsPerInbox: Long,
    /**
     * Null means the client keeps its compiled default. `false` is distinct from
     * absent, so an operator can switch the commit log off explicitly.
     */
    val commitLogEnabled: Boolean?,
) {
    internal companion object {
        internal fun fromFfi(mls: FfiMlsConfiguration): MlsConfiguration =
            MlsConfiguration(
                maxGroupMembers = mls.maxGroupMembers.published("mls.maxGroupMembers"),
                maxInstallationsPerInbox = mls.maxInstallationsPerInbox.published("mls.maxInstallationsPerInbox"),
                commitLogEnabled = mls.commitLogEnabled,
            )
    }
}

/**
 * One immutable snapshot of what a deployment published about itself
 * (spec 006 §5.2).
 *
 * [Client.serverConfiguration] returns the snapshot the client resolved at
 * build, [Client.refreshServerConfiguration] fetches a fresh one, and
 * [Client.fetchServerConfiguration] reads a deployment with no database and no
 * client at all.
 *
 * Counts the wire publishes as `uint64` are [Long] here, per spec 006 §7. Every
 * published value is far below 2^53, so the narrowing is lossless; a value that
 * would not fit throws [XMTPException] rather than wrapping silently. The four
 * `uint32` rates in [LimitsConfiguration] stay [UInt], which holds every value
 * that wire type can carry.
 */
data class ServerConfiguration(
    /** Stable operator-chosen name. Empty only before a first fetch succeeds. */
    val identifier: String,
    val serverVersion: String,
    /** Empty when the operator published no minimum. */
    val minLibxmtpVersion: String,
    val auth: AuthConfiguration,
    val retention: RetentionConfiguration,
    val limits: LimitsConfiguration,
    val mls: MlsConfiguration,
    /**
     * CAIP-2 chain ids this deployment verifies smart contract wallet signatures
     * on. Empty rejects every app-supplied signature.
     */
    val smartContractWalletChains: List<String>,
) {
    internal companion object {
        // implements: CONF-061
        internal fun fromFfi(configuration: FfiServerConfiguration): ServerConfiguration =
            ServerConfiguration(
                identifier = configuration.identifier,
                serverVersion = configuration.serverVersion,
                minLibxmtpVersion = configuration.minLibxmtpVersion,
                auth = AuthConfiguration.fromFfi(configuration.auth),
                retention = RetentionConfiguration.fromFfi(configuration.retention),
                limits = LimitsConfiguration.fromFfi(configuration.limits),
                mls = MlsConfiguration.fromFfi(configuration.mls),
                smartContractWalletChains = configuration.smartContractWalletChains,
            )
    }
}

/**
 * Narrow a published `uint64` to the [Long] spec 006 §7 maps it to.
 *
 * Every value a backend may publish is bounded well below 2^53, so this never
 * rejects a real configuration. A value that does not fit is a broken deployment
 * or a corrupt stored copy, and saying so beats handing an app a negative limit.
 */
internal fun ULong.published(field: String): Long {
    if (this > Long.MAX_VALUE.toULong()) {
        throw XMTPException("Server configuration $field is $this, which does not fit in a Long")
    }
    return toLong()
}
