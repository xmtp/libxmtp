import Foundation

// What one backend deployment publishes about itself (spec 006 §7).
//
// Every value here is a plain Swift mirror of the matching `Ffi*` record the
// uniffi binding returns. Counts the wire carries as `uint64` are ``UInt64``
// so no published value can be truncated; the four rate-limit fields the wire
// carries as `uint32` use the default `Int`. `optional bool` keeps its
// nullable form: absent means "the client keeps its compiled default", which
// is distinct from an explicit `false`.
//
// Read a snapshot with ``Client/serverConfiguration()``, read one deployment
// with no client at all with ``Client/fetchServerConfiguration(url:appVersion:)``,
// and fetch a fresh copy with ``Client/refreshServerConfiguration()``.

/// Public identity of one accepted signing key. Never the key itself.
public struct SigningKeyDescription: Equatable, Sendable {
	public let kid: String
	public let alg: String

	init(_ ffi: FfiSigningKeyDescription) {
		kid = ffi.kid
		alg = ffi.alg
	}
}

/// What a client must present to be admitted. An app acts on ``enabled`` and
/// ``requiredScopes``; the rest is there for operator tooling.
public struct AuthConfiguration: Equatable, Sendable {
	public let enabled: Bool
	public let keys: [SigningKeyDescription]
	public let audiences: [String]
	public let issuers: [String]
	public let requiredScopes: [String]

	init(_ ffi: FfiAuthConfiguration) {
		enabled = ffi.enabled
		keys = ffi.keys.map(SigningKeyDescription.init)
		audiences = ffi.audiences
		issuers = ffi.issuers
		requiredScopes = ffi.requiredScopes
	}
}

/// How long the deployment keeps each payload kind, in seconds.
public struct RetentionConfiguration: Equatable, Sendable {
	public let groupMessageSeconds: UInt64
	public let welcomeSeconds: UInt64
	public let keyPackageSeconds: UInt64

	init(_ ffi: FfiRetentionConfiguration) {
		groupMessageSeconds = ffi.groupMessageSeconds
		welcomeSeconds = ffi.welcomeSeconds
		keyPackageSeconds = ffi.keyPackageSeconds
	}
}

/// Request shapes the deployment accepts. The client chunks its own work to
/// these values; an app reads them to size its own batches.
public struct LimitsConfiguration: Equatable, Sendable {
	public let maxEnvelopeBytes: UInt64
	public let maxRequestBytes: UInt64
	public let maxResponseBytes: UInt64
	public let maxPublishTopics: UInt64
	public let maxQueryTopics: UInt64
	public let maxQueryLimit: UInt64
	public let defaultQueryLimit: UInt64
	public let maxNewestMetadataTopics: UInt64
	public let maxNewestFullTopics: UInt64
	public let maxUpdateAdds: UInt64
	public let maxUpdateRemoves: UInt64
	public let maxStreamTopics: UInt64
	public let maxStaticTopics: UInt64
	public let maxLookupIdentifiers: UInt64
	public let maxScwSignatures: UInt64
	public let maxIdentityEntries: UInt64
	public let maxUpdateFramesPerSecond: Int
	public let maxUpdateBurst: Int
	public let maxPingFramesPerSecond: Int
	public let maxPingBurst: Int

	init(_ ffi: FfiLimitsConfiguration) {
		maxEnvelopeBytes = ffi.maxEnvelopeBytes
		maxRequestBytes = ffi.maxRequestBytes
		maxResponseBytes = ffi.maxResponseBytes
		maxPublishTopics = ffi.maxPublishTopics
		maxQueryTopics = ffi.maxQueryTopics
		maxQueryLimit = ffi.maxQueryLimit
		defaultQueryLimit = ffi.defaultQueryLimit
		maxNewestMetadataTopics = ffi.maxNewestMetadataTopics
		maxNewestFullTopics = ffi.maxNewestFullTopics
		maxUpdateAdds = ffi.maxUpdateAdds
		maxUpdateRemoves = ffi.maxUpdateRemoves
		maxStreamTopics = ffi.maxStreamTopics
		maxStaticTopics = ffi.maxStaticTopics
		maxLookupIdentifiers = ffi.maxLookupIdentifiers
		maxScwSignatures = ffi.maxScwSignatures
		maxIdentityEntries = ffi.maxIdentityEntries
		maxUpdateFramesPerSecond = Int(ffi.maxUpdateFramesPerSecond)
		maxUpdateBurst = Int(ffi.maxUpdateBurst)
		maxPingFramesPerSecond = Int(ffi.maxPingFramesPerSecond)
		maxPingBurst = Int(ffi.maxPingBurst)
	}
}

/// Advisory group shapes. The backend publishes them and does not enforce them.
public struct MlsConfiguration: Equatable, Sendable {
	public let maxGroupMembers: UInt64
	public let maxInstallationsPerInbox: UInt64
	/// `nil` means the client keeps its compiled default. `false` is distinct
	/// from `nil`, so an operator can switch the commit log off explicitly.
	public let commitLogEnabled: Bool?

	init(_ ffi: FfiMlsConfiguration) {
		maxGroupMembers = ffi.maxGroupMembers
		maxInstallationsPerInbox = ffi.maxInstallationsPerInbox
		commitLogEnabled = ffi.commitLogEnabled
	}
}

/// One immutable snapshot of what a deployment published.
public struct ServerConfiguration: Equatable, Sendable {
	/// Stable operator-chosen name. Empty only before a first fetch succeeds.
	public let identifier: String
	public let serverVersion: String
	/// Empty when the operator published no minimum.
	public let minLibxmtpVersion: String
	public let auth: AuthConfiguration
	public let retention: RetentionConfiguration
	public let limits: LimitsConfiguration
	public let mls: MlsConfiguration
	/// CAIP-2 chain ids this deployment verifies smart contract wallet
	/// signatures on. Empty rejects every app-supplied signature.
	public let smartContractWalletChains: [String]

	// implements: CONF-061
	init(_ ffi: FfiServerConfiguration) {
		identifier = ffi.identifier
		serverVersion = ffi.serverVersion
		minLibxmtpVersion = ffi.minLibxmtpVersion
		auth = AuthConfiguration(ffi.auth)
		retention = RetentionConfiguration(ffi.retention)
		limits = LimitsConfiguration(ffi.limits)
		mls = MlsConfiguration(ffi.mls)
		smartContractWalletChains = ffi.smartContractWalletChains
	}
}

/// Bridge to the generated module-level configuration RPC.
///
/// `Client.fetchServerConfiguration` shadows the generated global name inside
/// the `Client` scope, so the call is made from module scope here, where the
/// generated function is the only candidate.
func fetchFfiServerConfiguration(
	backendUrl: String, appVersion: String?
) async throws -> FfiServerConfiguration {
	try await fetchServerConfiguration(
		backendUrl: backendUrl, appVersion: appVersion
	)
}
