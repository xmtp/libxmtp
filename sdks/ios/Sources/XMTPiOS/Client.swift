import Foundation
import os

/// A callback invoked before a key authentication event, giving the app a chance to
/// prepare the UI (e.g., show a signing prompt) before the user is asked to sign.
public typealias PreEventCallback = () async throws -> Void

/// Metadata associated with a message, such as sender and timestamp information.
///
/// This is a type alias for the FFI-layer `FfiMessageMetadata`.
public typealias MessageMetadata = FfiMessageMetadata

/// Errors thrown by ``Client`` during creation, identity resolution, or configuration.
public enum ClientError: Error, CustomStringConvertible, LocalizedError {
	/// Client creation failed. The associated value contains a human-readable reason.
	case creationError(String)
	/// No inbox ID could be found or generated for the given identity.
	case missingInboxId
	/// The provided inbox ID is invalid (e.g., it starts with `0x`).
	case invalidInboxId(String)

	public var description: String {
		switch self {
		case let .creationError(err):
			"ClientError.creationError: \(err)"
		case .missingInboxId:
			"ClientError.missingInboxId"
		case let .invalidInboxId(inboxId):
			"Invalid inboxId: \(inboxId). Inbox IDs cannot start with '0x'."
		}
	}

	public var errorDescription: String? {
		description
	}
}

/// Controls which groups are eligible for automatic MLS fork recovery.
///
/// An MLS "fork" occurs when group state diverges between members. Recovery re-establishes
/// a consistent state so messages can flow again.
public enum ForkRecoveryPolicy {
	/// Disable fork recovery entirely.
	case none
	/// Only recover groups that appear in ``ForkRecoveryOptions/groupsToRequestRecovery``.
	case allowlistedGroups
	/// Attempt recovery for every forked group.
	case all

	func toFfi() -> FfiForkRecoveryPolicy {
		switch self {
		case .none:
			.none
		case .allowlistedGroups:
			.allowlistedGroups
		case .all:
			.all
		}
	}
}

/// Configuration for MLS fork recovery behavior.
///
/// Use this struct to control whether and how the client requests and responds
/// to fork-recovery operations.
public struct ForkRecoveryOptions {
	/// The policy that determines which groups may request fork recovery.
	public var enableRecoveryRequests: ForkRecoveryPolicy
	/// Group IDs (hex strings) eligible for recovery when the policy is ``ForkRecoveryPolicy/allowlistedGroups``.
	public var groupsToRequestRecovery: [String]
	/// When `true`, this installation will not respond to fork-recovery requests from other members.
	public var disableRecoveryResponses: Bool?
	/// Optional interval, in nanoseconds, between recovery worker runs.
	public var workerIntervalNs: UInt64?

	/// Creates fork recovery options.
	///
	/// - Parameters:
	///   - enableRecoveryRequests: The policy for which groups may request recovery.
	///   - groupsToRequestRecovery: Group IDs eligible for recovery (used with ``ForkRecoveryPolicy/allowlistedGroups``).
	///   - disableRecoveryResponses: Pass `true` to prevent this installation from responding to recovery requests.
	///   - workerIntervalNs: Optional interval in nanoseconds for the recovery worker.
	public init(
		enableRecoveryRequests: ForkRecoveryPolicy,
		groupsToRequestRecovery: [String],
		disableRecoveryResponses: Bool? = nil,
		workerIntervalNs: UInt64? = nil
	) {
		self.enableRecoveryRequests = enableRecoveryRequests
		self.groupsToRequestRecovery = groupsToRequestRecovery
		self.disableRecoveryResponses = disableRecoveryResponses
		self.workerIntervalNs = workerIntervalNs
	}

	func toFfi() -> FfiForkRecoveryOpts {
		FfiForkRecoveryOpts(
			enableRecoveryRequests: enableRecoveryRequests.toFfi(),
			groupsToRequestRecovery: groupsToRequestRecovery,
			disableRecoveryResponses: disableRecoveryResponses,
			workerIntervalNs: workerIntervalNs
		)
	}
}

/// Specify configuration options for creating a ``Client``.
public struct ClientOptions {
	/// Specify network options
	public struct Api {
		/// Specify which XMTP network to connect to. Defaults to ``.dev``
		public var env: XMTPEnvironment = .dev

		/// Specify whether the API client should use TLS security. In general this should only be false when using the
		/// `.local` environment.
		public var isSecure = true

		public var appVersion: String?

		/// Future proofing - gateway URL support.
		public var gatewayHost: String?

		public init(
			env: XMTPEnvironment = .dev, isSecure: Bool = true,
			appVersion: String? = nil,
			gatewayHost: String? = nil
		) {
			self.env = env
			self.isSecure = isSecure
			self.appVersion = appVersion
			self.gatewayHost = gatewayHost
		}
	}

	public var api = Api()
	public var codecs: [any ContentCodec] = []

	/// `preAuthenticateToInboxCallback` will be called immediately before an Auth Inbox signature is requested from the
	/// user
	public var preAuthenticateToInboxCallback: PreEventCallback?

	public var dbEncryptionKey: Data
	public var dbDirectory: String?
	public var deviceSyncEnabled: Bool
	public var debugEventsEnabled: Bool
	public var forkRecoveryOptions: ForkRecoveryOptions?

	public init(
		api: Api = Api(),
		codecs: [any ContentCodec] = [],
		preAuthenticateToInboxCallback: PreEventCallback? = nil,
		dbEncryptionKey: Data,
		dbDirectory: String? = nil,
		deviceSyncEnabled: Bool = true,
		debugEventsEnabled: Bool = false,
		forkRecoveryOptions: ForkRecoveryOptions? = nil
	) {
		self.api = api
		self.codecs = codecs
		self.preAuthenticateToInboxCallback = preAuthenticateToInboxCallback
		self.dbEncryptionKey = dbEncryptionKey
		self.dbDirectory = dbDirectory
		self.deviceSyncEnabled = deviceSyncEnabled
		self.debugEventsEnabled = debugEventsEnabled
		self.forkRecoveryOptions = forkRecoveryOptions
	}
}

struct ApiCacheKey {
	let api: ClientOptions.Api

	var stringValue: String {
		"\(api.env.url)|\(api.isSecure)|\(api.appVersion ?? "nil")|\(api.gatewayHost ?? "nil")"
	}
}

// To be removed in a future release
actor ApiClientCache {
	private var apiClientCache: [String: XmtpApiClient] = [:]
	private var syncApiClientCache: [String: XmtpApiClient] = [:]

	func getClient(forKey key: String) -> XmtpApiClient? {
		apiClientCache[key]
	}

	func setClient(_ client: XmtpApiClient, forKey key: String) {
		apiClientCache[key] = client
	}

	func getSyncClient(forKey key: String) -> XmtpApiClient? {
		syncApiClientCache[key]
	}

	func setSyncClient(_ client: XmtpApiClient, forKey key: String) {
		syncApiClientCache[key] = client
	}
}

/// A unique, network-assigned identifier for an XMTP inbox.
///
/// An inbox ID is a hex-encoded string that represents a user's identity on the XMTP network.
/// Multiple wallet addresses (accounts) can be associated with a single inbox ID.
public typealias InboxId = String

/// The primary entry point for interacting with the XMTP messaging network.
///
/// A `Client` represents a single XMTP installation. It manages the local encrypted
/// database, network connections, conversations, and identity operations.
///
/// ## Creating a Client
///
/// Use ``create(account:options:)`` to register a new identity, or ``build(publicIdentity:options:inboxId:)``
/// to reconnect an existing one:
///
/// ```swift
/// // First launch -- registers on the network
/// let client = try await Client.create(account: wallet, options: opts)
///
/// // Subsequent launches -- no signing key needed
/// let client = try await Client.build(publicIdentity: identity, options: opts)
/// ```
///
/// ## Thread Safety
///
/// `Client` is a `final class` and is safe to share across Swift concurrency contexts.
public final class Client {
	/// The inbox ID associated with this client's identity on the XMTP network.
	public let inboxID: InboxId
	/// The version string of the underlying libxmtp library.
	public let libXMTPVersion: String = getVersionInfo()
	/// The file-system path to the local encrypted SQLite database.
	public let dbPath: String
	/// A hex-encoded identifier for this specific app installation.
	public let installationID: String
	/// The public identity (e.g., wallet address) associated with this client.
	public let publicIdentity: PublicIdentity
	/// The XMTP network environment this client is connected to (e.g., `.dev`, `.production`).
	public let environment: XMTPEnvironment
	private let ffiClient: FfiXmtpClient
	private static let apiCache = ApiClientCache()

	/// The entry point for listing, creating, and streaming conversations.
	public lazy var conversations: Conversations = .init(
		client: self, ffiConversations: ffiClient.conversations(),
		ffiClient: ffiClient
	)

	/// Manages consent and private preference settings for this inbox.
	public lazy var preferences: PrivatePreferences = .init(
		client: self, ffiClient: ffiClient
	)

	/// Provides access to debug and diagnostic information for this client instance.
	public lazy var debugInformation: XMTPDebugInformation = .init(
		client: self, ffiClient: ffiClient
	)

	static var codecRegistry = CodecRegistry()

	/// Registers a content codec globally so all clients can encode and decode its content type.
	///
	/// Call this before sending or receiving messages that use a custom content type.
	///
	/// - Parameter codec: The codec to register.
	public static func register(codec: any ContentCodec) {
		codecRegistry.register(codec: codec)
	}

	static func initializeClient(
		publicIdentity: PublicIdentity,
		options: ClientOptions,
		signingKey: SigningKey?,
		inboxId: InboxId,
		apiClient _: XmtpApiClient? = nil,
		buildOffline: Bool = false
	) async throws -> Client {
		let (libxmtpClient, dbPath) = try await initFFiClient(
			accountIdentifier: publicIdentity,
			options: options,
			inboxId: inboxId,
			buildOffline: buildOffline
		)

		let client = try Client(
			ffiClient: libxmtpClient,
			dbPath: dbPath,
			installationID: libxmtpClient.installationId().toHex,
			inboxID: libxmtpClient.inboxId(),
			environment: options.api.env,
			publicIdentity: publicIdentity
		)

		try await options.preAuthenticateToInboxCallback?()
		if let signatureRequest = client.ffiClient.signatureRequest() {
			if let signingKey {
				do {
					try await handleSignature(
						for: signatureRequest, signingKey: signingKey
					)
					try await client.ffiClient.registerIdentity(
						signatureRequest: signatureRequest
					)
				} catch {
					throw ClientError.creationError(
						"Failed to sign the message: \(error.localizedDescription)"
					)
				}
			} else {
				// add log messages here for logging 1) dbDirectory, 2) number of files in dbDirectory, 3) dbPath
				let dbPathDirectory = URL(fileURLWithPath: dbPath)
					.deletingLastPathComponent().path
				XMTPLogger.database.error(
					"custom dbDirectory: \(options.dbDirectory ?? "nil")"
				)
				XMTPLogger.database.error("dbPath: \(dbPath)")
				XMTPLogger.database.error(
					"dbPath Directory: \(dbPathDirectory)"
				)
				XMTPLogger.database.error(
					"Number of files in dbDirectory: \(getNumberOfFilesInDirectory(directory: dbPathDirectory))"
				)
				throw ClientError.creationError(
					"No signing key found, you must pass a SigningKey in order to create an MLS client"
				)
			}
		}

		// Register codecs
		for codec in options.codecs {
			register(codec: codec)
		}

		return client
	}

	/// Creates a new XMTP client and registers it on the network.
	///
	/// This is the primary entry point for first-time users. It looks up (or generates) an
	/// inbox ID for the given account, creates the local encrypted database, and registers the
	/// installation's identity on the XMTP network. The `account` will be asked to produce
	/// a signature during this process.
	///
	/// ```swift
	/// let options = ClientOptions(dbEncryptionKey: myKey)
	/// let client = try await Client.create(account: wallet, options: options)
	/// print(client.inboxID) // the newly registered inbox
	/// ```
	///
	/// On subsequent app launches where the identity is already registered, use
	/// ``build(publicIdentity:options:inboxId:)`` instead -- it does not require a signing key.
	///
	/// - Parameters:
	///   - account: A signing key (e.g., an Ethereum wallet) used to authenticate and register the identity.
	///   - options: Configuration for network, database, and codecs.
	/// - Returns: A fully initialized and network-registered ``Client``.
	/// - Throws: ``ClientError/creationError(_:)`` if signing or registration fails.
	public static func create(
		account: SigningKey, options: ClientOptions
	)
		async throws -> Client
	{
		let identity = account.identity
		let inboxId = try await getOrCreateInboxId(
			api: options.api, publicIdentity: identity
		)

		return try await initializeClient(
			publicIdentity: identity,
			options: options,
			signingKey: account,
			inboxId: inboxId
		)
	}

	/// Builds a client for an already-registered identity without requiring a signing key.
	///
	/// Use this on subsequent app launches after the identity has been registered with
	/// ``create(account:options:)``. Because no signing key is needed, the user is not
	/// prompted to sign anything.
	///
	/// ```swift
	/// // Reconnect on app launch
	/// let client = try await Client.build(
	///     publicIdentity: savedIdentity,
	///     options: options
	/// )
	/// ```
	///
	/// If you supply an `inboxId`, the client will attempt to build offline without
	/// contacting the network, which is useful for immediate access in poor connectivity.
	///
	/// - Parameters:
	///   - publicIdentity: The public identity (e.g., wallet address) previously registered.
	///   - options: Configuration for network, database, and codecs.
	///   - inboxId: An optional inbox ID. When provided, the client builds offline without a network lookup.
	/// - Returns: A reconnected ``Client`` backed by the existing local database.
	/// - Throws: ``ClientError/creationError(_:)`` if the local database cannot be opened
	///   or the identity is not yet registered.
	public static func build(
		publicIdentity: PublicIdentity, options: ClientOptions,
		inboxId: InboxId? = nil
	)
		async throws -> Client
	{
		let resolvedInboxId: String = if let existingInboxId = inboxId {
			existingInboxId
		} else {
			try await getOrCreateInboxId(
				api: options.api, publicIdentity: publicIdentity
			)
		}

		return try await initializeClient(
			publicIdentity: publicIdentity,
			options: options,
			signingKey: nil,
			inboxId: resolvedInboxId,
			buildOffline: inboxId != nil
		)
	}

	/// Creates a raw FFI client without signing or registering the identity.
	///
	/// - Note: Deprecated. Use ``create(account:options:)`` instead, which handles
	///   signing and identity registration automatically. For reconnecting a
	///   previously registered identity without a signer, use ``build(publicIdentity:options:inboxId:)``.
	@available(
		*,
		deprecated,
		message: """
		This function is delicate and should be used with caution.
		Creating an FfiClient without signing or registering will create a broken experience.
		Use `create()` instead.
		"""
	)
	public static func ffiCreateClient(
		identity: PublicIdentity, clientOptions: ClientOptions
	) async throws -> Client {
		let recoveredInboxId = try await getOrCreateInboxId(
			api: clientOptions.api, publicIdentity: identity
		)

		let (ffiClient, dbPath) = try await initFFiClient(
			accountIdentifier: identity,
			options: clientOptions,
			inboxId: recoveredInboxId
		)

		return try Client(
			ffiClient: ffiClient,
			dbPath: dbPath,
			installationID: ffiClient.installationId().toHex,
			inboxID: ffiClient.inboxId(),
			environment: clientOptions.api.env,
			publicIdentity: identity
		)
	}

	private static func initFFiClient(
		accountIdentifier: PublicIdentity,
		options: ClientOptions,
		inboxId: InboxId,
		buildOffline: Bool = false
	) async throws -> (FfiXmtpClient, String) {
		let mlsDbDirectory = options.dbDirectory
		var directoryURL: URL
		if let mlsDbDirectory {
			let fileManager = FileManager.default
			directoryURL = URL(
				fileURLWithPath: mlsDbDirectory, isDirectory: true
			)
			// Check if the directory exists, if not, create it
			if !fileManager.fileExists(atPath: directoryURL.path) {
				do {
					try fileManager.createDirectory(
						at: directoryURL, withIntermediateDirectories: true,
						attributes: nil
					)
				} catch {
					throw ClientError.creationError(
						"Failed db directory \(mlsDbDirectory)"
					)
				}
			}
		} else {
			directoryURL = URL.documentsDirectory
		}

		let alias = "xmtp-\(options.api.env.rawValue)-\(inboxId).db3"
		var dbURL = directoryURL.appendingPathComponent(alias).path
		var fileExists = FileManager.default.fileExists(atPath: dbURL)

		if !fileExists {
			let legacyAlias =
				"xmtp-\(options.api.env.legacyRawValue)-\(inboxId).db3"
			let legacyDbURL = directoryURL.appendingPathComponent(legacyAlias)
				.path
			let legacyFileExists = FileManager.default.fileExists(
				atPath: legacyDbURL
			)

			if legacyFileExists {
				dbURL = legacyDbURL
			}
		}

		let deviceSyncMode: FfiDeviceSyncMode =
			!options.deviceSyncEnabled ? .disabled : .enabled

		let ffiClient = try await createClient(
			api: connectToApiBackend(api: options.api),
			syncApi: connectToSyncApiBackend(api: options.api),
			db: DbOptions(db: dbURL, encryptionKey: options.dbEncryptionKey, maxDbPoolSize: nil, minDbPoolSize: nil),
			inboxId: inboxId,
			accountIdentifier: accountIdentifier.ffiPrivate,
			nonce: 0,
			legacySignedPrivateKeyProto: nil,
			deviceSyncMode: deviceSyncMode,
			allowOffline: buildOffline,
			forkRecoveryOpts: options.forkRecoveryOptions?.toFfi()
		)

		return (ffiClient, dbURL)
	}

	private static func handleSignature(
		for signatureRequest: FfiSignatureRequest, signingKey: SigningKey
	) async throws {
		let signedData = try await signingKey.sign(
			signatureRequest.signatureText()
		)

		switch signingKey.type {
		case .SCW:
			guard let chainId = signingKey.chainId else {
				throw ClientError.creationError(
					"Chain id must be present to sign Smart Contract Wallet"
				)
			}
			try await signatureRequest.addScwSignature(
				signatureBytes: signedData.rawData,
				address: signingKey.identity.identifier,
				chainId: UInt64(chainId),
				blockNumber: signingKey.blockNumber.map { UInt64($0) }
			)

		case .EOA:
			try await signatureRequest.addEcdsaSignature(
				signatureBytes: signedData.rawData
			)
		}
	}

	/// Connects (or returns a cached connection) to the XMTP API backend.
	///
	/// - Parameter api: Network configuration specifying the environment and TLS settings.
	/// - Returns: A connected ``XmtpApiClient``.
	/// - Throws: If the connection cannot be established.
	public static func connectToApiBackend(api: ClientOptions.Api) async throws
		-> XmtpApiClient
	{
		let cacheKey = ApiCacheKey(api: api).stringValue

		// Check for an existing connected client
		if let cached = await apiCache.getClient(forKey: cacheKey),
		   try await isConnected(api: cached)
		{
			return cached
		}

		// Either not cached or not connected; create new client
		let newClient = try await connectToBackend(
			v3Host: api.env.url,
			gatewayHost: api.gatewayHost,
			isSecure: api.isSecure,
			clientMode: FfiClientMode.default,
			appVersion: api.appVersion,
			authCallback: nil,
			authHandle: nil
		)
		await apiCache.setClient(newClient, forKey: cacheKey)
		return newClient
	}

	/// Connects (or returns a cached connection) to the XMTP sync API backend used for device sync.
	///
	/// - Parameter api: Network configuration specifying the environment and TLS settings.
	/// - Returns: A connected ``XmtpApiClient`` for sync operations.
	/// - Throws: If the connection cannot be established.
	public static func connectToSyncApiBackend(api: ClientOptions.Api)
		async throws
		-> XmtpApiClient
	{
		let cacheKey = ApiCacheKey(api: api).stringValue

		// Check for an existing connected client
		if let cached = await apiCache.getSyncClient(forKey: cacheKey),
		   try await isConnected(api: cached)
		{
			return cached
		}

		// Either not cached or not connected; create new client
		let newClient = try await connectToBackend(
			v3Host: api.env.url,
			gatewayHost: api.gatewayHost,
			isSecure: api.isSecure,
			clientMode: FfiClientMode.default,
			appVersion: api.appVersion,
			authCallback: nil,
			authHandle: nil
		)
		await apiCache.setSyncClient(newClient, forKey: cacheKey)
		return newClient
	}

	/// Looks up the inbox ID for a public identity on the network, or generates a new one if none exists.
	///
	/// - Parameters:
	///   - api: Network configuration.
	///   - publicIdentity: The identity to look up.
	/// - Returns: The existing or newly generated ``InboxId``.
	public static func getOrCreateInboxId(
		api: ClientOptions.Api, publicIdentity: PublicIdentity
	) async throws -> InboxId {
		var inboxId: String
		do {
			inboxId =
				try await getInboxIdForIdentifier(
					api: connectToApiBackend(api: api),
					accountIdentifier: publicIdentity.ffiPrivate
				)
				?? generateInboxId(
					accountIdentifier: publicIdentity.ffiPrivate, nonce: 0
				)
		} catch {
			inboxId = try generateInboxId(
				accountIdentifier: publicIdentity.ffiPrivate, nonce: 0
			)
		}
		return inboxId
	}

	/// Revokes one or more installations from an inbox without requiring a local client.
	///
	/// This is a static convenience for revoking installations when you only have
	/// the signing key and inbox ID, without a fully constructed ``Client``.
	///
	/// - Parameters:
	///   - api: Network configuration.
	///   - signingKey: The recovery signing key that owns the inbox.
	///   - inboxId: The inbox ID to revoke installations from.
	///   - installationIds: Hex-encoded installation IDs to revoke.
	/// - Throws: ``ClientError/creationError(_:)`` if signing or the network request fails.
	public static func revokeInstallations(
		api: ClientOptions.Api,
		signingKey: SigningKey,
		inboxId: InboxId,
		installationIds: [String]
	) async throws {
		let apiClient = try await connectToApiBackend(api: api)
		let rootIdentity = signingKey.identity.ffiPrivate
		let ids = installationIds.map(\.hexToData)
		let signatureRequest: FfiSignatureRequest
		#if canImport(XMTPiOS)
			signatureRequest = try await XMTPiOS.revokeInstallations(
				api: apiClient, recoveryIdentifier: rootIdentity, inboxId: inboxId,
				installationIds: ids
			)
		#else
			signatureRequest = try await XMTP.revokeInstallations(
				api: apiClient, recoveryIdentifier: rootIdentity, inboxId: inboxId,
				installationIds: ids
			)
		#endif
		do {
			try await Client.handleSignature(
				for: signatureRequest,
				signingKey: signingKey
			)
			try await applySignatureRequest(
				api: apiClient, signatureRequest: signatureRequest
			)
		} catch {
			throw ClientError.creationError(
				"Failed to sign the message: \(error.localizedDescription)"
			)
		}
	}

	/// Applies a pre-built signature request to the network without a local client.
	///
	/// - Note: Deprecated. Use ``revokeInstallations(api:signingKey:inboxId:installationIds:)`` instead,
	///   which manages the signature flow automatically.
	@available(
		*,
		deprecated,
		message: """
		This function is delicate and should be used with caution.
		Should only be used if trying to manage the signature flow independently;
		otherwise use `revokeInstallations()` instead.
		"""
	)
	public static func ffiApplySignatureRequest(
		api: ClientOptions.Api,
		signatureRequest: SignatureRequest
	)
		async throws
	{
		let apiClient = try await connectToApiBackend(api: api)
		try await applySignatureRequest(
			api: apiClient,
			signatureRequest: signatureRequest.ffiSignatureRequest
		)
	}

	/// Creates a revocation signature request for manual signature management.
	///
	/// - Note: Deprecated. Use ``revokeInstallations(api:signingKey:inboxId:installationIds:)`` instead,
	///   which manages the signature flow automatically.
	@available(
		*,
		deprecated,
		message: """
		This function is delicate and should be used with caution.
		Should only be used if trying to manage the signature flow independently;
		otherwise use `revokeInstallations()` instead.
		"""
	)
	public static func ffiRevokeInstallations(
		api: ClientOptions.Api,
		publicIdentity: PublicIdentity,
		inboxId: InboxId,
		installationIds: [String]
	) async throws
		-> SignatureRequest
	{
		let apiClient = try await connectToApiBackend(api: api)
		let rootIdentity = publicIdentity.ffiPrivate
		let ids = installationIds.map(\.hexToData)
		let signatureRequest: FfiSignatureRequest
		#if canImport(XMTPiOS)
			signatureRequest = try await XMTPiOS.revokeInstallations(
				api: apiClient, recoveryIdentifier: rootIdentity, inboxId: inboxId,
				installationIds: ids
			)
		#else
			signatureRequest = try await XMTP.revokeInstallations(
				api: apiClient, recoveryIdentifier: rootIdentity, inboxId: inboxId,
				installationIds: ids
			)
		#endif
		return SignatureRequest(ffiSignatureRequest: signatureRequest)
	}

	private static func prepareClient(
		api: ClientOptions.Api,
		identity: PublicIdentity = PublicIdentity(
			kind: .ethereum,
			identifier: "0x0000000000000000000000000000000000000000"
		)
	) async throws -> FfiXmtpClient {
		let inboxId = try await getOrCreateInboxId(
			api: api, publicIdentity: identity
		)
		return try await createClient(
			api: connectToApiBackend(api: api),
			syncApi: connectToApiBackend(api: api),
			db: DbOptions(db: nil, encryptionKey: nil, maxDbPoolSize: nil, minDbPoolSize: nil),
			inboxId: inboxId,
			accountIdentifier: identity.ffiPrivate,
			nonce: 0,
			legacySignedPrivateKeyProto: nil,
			deviceSyncMode: nil,
			allowOffline: false,
			forkRecoveryOpts: nil
		)
	}

	/// Checks whether one or more identities are reachable on the XMTP network without a local client.
	///
	/// - Parameters:
	///   - accountIdentities: The identities to check.
	///   - api: Network configuration.
	/// - Returns: A dictionary mapping each identity's ``PublicIdentity/identifier`` to a `Bool`
	///   indicating whether that identity has an active XMTP installation.
	public static func canMessage(
		accountIdentities: [PublicIdentity], api: ClientOptions.Api
	) async throws -> [String: Bool] {
		let ffiClient = try await prepareClient(api: api)
		let ffiIdentifiers = accountIdentities.map(\.ffiPrivate)
		let result = try await ffiClient.canMessage(
			accountIdentifiers: ffiIdentifiers
		)

		return Dictionary(
			uniqueKeysWithValues: result.map { ($0.key.identifier, $0.value) }
		)
	}

	/// Fetches the identity state for a list of inbox IDs from the network without a local client.
	///
	/// - Parameters:
	///   - inboxIds: The inbox IDs to query.
	///   - api: Network configuration.
	/// - Returns: An array of ``InboxState`` values describing each inbox's installations and linked accounts.
	public static func inboxStatesForInboxIds(
		inboxIds: [InboxId],
		api: ClientOptions.Api
	) async throws -> [InboxState] {
		let apiClient = try await connectToApiBackend(api: api)
		let result = try await inboxStateFromInboxIds(
			api: apiClient, inboxIds: inboxIds
		)
		return result.map { InboxState(ffiInboxState: $0) }
	}

	/// Retrieves the MLS key package status for each installation ID without a local client.
	///
	/// Key package status indicates whether an installation's MLS key material is valid, expired, or revoked.
	///
	/// - Parameters:
	///   - installationIds: Hex-encoded installation IDs to query.
	///   - api: Network configuration.
	/// - Returns: A dictionary mapping each hex-encoded installation ID to its ``FfiKeyPackageStatus``.
	public static func keyPackageStatusesForInstallationIds(
		installationIds: [String],
		api: ClientOptions.Api
	) async throws -> [String: FfiKeyPackageStatus] {
		let ffiClient = try await prepareClient(api: api)

		let byteArrays = installationIds.map(\.hexToData)
		let result =
			try await ffiClient.getKeyPackageStatusesForInstallationIds(
				installationIds: byteArrays
			)
		var statusMap: [String: FfiKeyPackageStatus] = [:]
		for (keyBytes, status) in result {
			let keyHex = keyBytes.toHex
			statusMap[keyHex] = status
		}
		return statusMap
	}

	/// Fetches the metadata of the newest message in each group without a local client.
	///
	/// Useful for displaying preview information (sender, timestamp) without downloading full messages.
	///
	/// - Parameters:
	///   - groupIds: Hex-encoded group IDs to query.
	///   - api: Network configuration.
	/// - Returns: A dictionary mapping each hex-encoded group ID to its newest ``MessageMetadata``.
	public static func getNewestMessageMetadata(
		groupIds: [String],
		api: ClientOptions.Api
	) async throws -> [String: MessageMetadata] {
		let apiClient = try await connectToApiBackend(api: api)
		let groupIdData = groupIds.map(\.hexToData)
		let result: [Data: FfiMessageMetadata]
		#if canImport(XMTPiOS)
			result = try await XMTPiOS.getNewestMessageMetadata(
				api: apiClient,
				groupIds: groupIdData
			)
		#else
			result = try await XMTP.getNewestMessageMetadata(
				api: apiClient,
				groupIds: groupIdData
			)
		#endif
		return Dictionary(
			uniqueKeysWithValues: result.map { ($0.key.toHex, $0.value) }
		)
	}

	init(
		ffiClient: FfiXmtpClient, dbPath: String,
		installationID: String, inboxID: InboxId, environment: XMTPEnvironment,
		publicIdentity: PublicIdentity
	) throws {
		self.ffiClient = ffiClient
		self.dbPath = dbPath
		self.installationID = installationID
		self.inboxID = inboxID
		self.environment = environment
		self.publicIdentity = publicIdentity
	}

	/// Links an additional signing key (wallet) to this client's inbox.
	///
	/// - Note: Deprecated. Use ``addAccount(newAccount:allowReassignInboxId:)`` with care.
	///   Adding a wallet already associated with another inbox will cause that wallet to lose
	///   access to its original inbox. Call ``inboxIdFromIdentity(identity:)`` first to check
	///   whether the wallet is already linked elsewhere.
	@available(
		*, deprecated,
		message:
		"This function is delicate and should be used with caution. Adding a wallet already associated with an inboxId will cause the wallet to loose access to that inbox. See: inboxIdFromIdentity(publicIdentity)"
	)
	public func addAccount(
		newAccount: SigningKey, allowReassignInboxId: Bool = false
	)
		async throws
	{
		let inboxId: String? =
			allowReassignInboxId
				? nil : try await inboxIdFromIdentity(identity: newAccount.identity)

		if allowReassignInboxId || (inboxId?.isEmpty ?? true) {
			let signatureRequest = try await ffiAddIdentity(
				identityToAdd: newAccount.identity,
				allowReassignInboxId: allowReassignInboxId
			)
			do {
				try await Client.handleSignature(
					for: signatureRequest.ffiSignatureRequest,
					signingKey: newAccount
				)
				try await ffiApplySignatureRequest(
					signatureRequest: signatureRequest
				)
			} catch {
				throw ClientError.creationError(
					"Failed to sign the message: \(error.localizedDescription)"
				)
			}
		} else {
			throw ClientError.creationError(
				"This wallet is already associated with inbox \(inboxId ?? "Unknown")"
			)
		}
	}

	/// Unlinks an identity from this inbox, signed by the recovery account.
	///
	/// After removal, the identity will no longer be associated with this inbox and
	/// cannot send or receive messages through it.
	///
	/// - Parameters:
	///   - recoveryAccount: The signing key authorized to make identity changes on this inbox.
	///   - identityToRemove: The public identity to unlink.
	/// - Throws: ``ClientError/creationError(_:)`` if signing fails.
	public func removeAccount(
		recoveryAccount: SigningKey, identityToRemove: PublicIdentity
	) async throws {
		let signatureRequest = try await ffiRevokeIdentity(
			identityToRemove: identityToRemove
		)
		do {
			try await Client.handleSignature(
				for: signatureRequest.ffiSignatureRequest,
				signingKey: recoveryAccount
			)
			try await ffiApplySignatureRequest(
				signatureRequest: signatureRequest
			)
		} catch {
			throw ClientError.creationError(
				"Failed to sign the message: \(error.localizedDescription)"
			)
		}
	}

	/// Revokes every installation on this inbox except the current one.
	///
	/// This is useful when a user wants to sign out of all other devices.
	/// If there are no other installations, this method returns without error.
	///
	/// - Parameter signingKey: The signing key authorized to make identity changes.
	/// - Throws: ``ClientError/creationError(_:)`` if signing fails.
	public func revokeAllOtherInstallations(signingKey: SigningKey) async throws {
		guard let signatureRequest = try await ffiRevokeAllOtherInstallations() else {
			// No other installations to revoke – nothing to do.
			return
		}
		do {
			try await Client.handleSignature(
				for: signatureRequest.ffiSignatureRequest,
				signingKey: signingKey
			)
			try await ffiApplySignatureRequest(
				signatureRequest: signatureRequest
			)
		} catch {
			throw ClientError.creationError(
				"Failed to sign the message: \(error.localizedDescription)"
			)
		}
	}

	/// Revokes specific installations from this inbox.
	///
	/// - Parameters:
	///   - signingKey: The signing key authorized to make identity changes.
	///   - installationIds: Hex-encoded installation IDs to revoke.
	/// - Throws: ``ClientError/creationError(_:)`` if signing fails.
	public func revokeInstallations(
		signingKey: SigningKey, installationIds: [String]
	) async throws {
		let installations = installationIds.map(\.hexToData)
		let signatureRequest = try await ffiRevokeInstallations(
			ids: installations
		)
		do {
			try await Client.handleSignature(
				for: signatureRequest.ffiSignatureRequest,
				signingKey: signingKey
			)
			try await ffiApplySignatureRequest(
				signatureRequest: signatureRequest
			)
		} catch {
			throw ClientError.creationError(
				"Failed to sign the message: \(error.localizedDescription)"
			)
		}
	}

	/// Checks whether a single identity is reachable on the XMTP network.
	///
	/// - Parameter identity: The identity to check.
	/// - Returns: `true` if the identity has an active XMTP installation, `false` otherwise.
	public func canMessage(identity: PublicIdentity) async throws -> Bool {
		let canMessage = try await canMessage(identities: [
			identity,
		])
		return canMessage[identity.identifier] ?? false
	}

	/// Checks whether multiple identities are reachable on the XMTP network.
	///
	/// - Parameter identities: The identities to check.
	/// - Returns: A dictionary mapping each identity's ``PublicIdentity/identifier`` to a `Bool`
	///   indicating reachability.
	public func canMessage(identities: [PublicIdentity]) async throws
		-> [String: Bool]
	{
		let ffiIdentifiers = identities.map(\.ffiPrivate)
		let result = try await ffiClient.canMessage(
			accountIdentifiers: ffiIdentifiers
		)

		return Dictionary(
			uniqueKeysWithValues: result.map { ($0.key.identifier, $0.value) }
		)
	}

	/// Deletes the local encrypted database file from disk.
	///
	/// - Important: This is a destructive, irreversible operation. All locally cached
	///   conversations and messages will be lost. The client instance should not be used after
	///   calling this method. You will need to call ``create(account:options:)`` or
	///   ``build(publicIdentity:options:inboxId:)`` to obtain a new client.
	public func deleteLocalDatabase() throws {
		try dropLocalDatabaseConnection()
		let fm = FileManager.default
		try fm.removeItem(atPath: dbPath)
	}

	/// Drops the active connection to the local database.
	///
	/// - Note: Deprecated. Avoid using this method directly. If you must drop the connection,
	///   always call ``reconnectLocalDatabase()`` before performing any further operations,
	///   or the app will error on the next database access.
	@available(
		*, deprecated,
		message:
		"This function is delicate and should be used with caution. App will error if database not properly reconnected. See: reconnectLocalDatabase()"
	)
	public func dropLocalDatabaseConnection() throws {
		try ffiClient.releaseDbConnection()
	}

	/// Re-establishes the connection to the local encrypted database after it was dropped.
	public func reconnectLocalDatabase() async throws {
		try await ffiClient.dbReconnect()
	}

	/// Looks up the inbox ID associated with a public identity on the network.
	///
	/// - Parameter identity: The identity to look up.
	/// - Returns: The ``InboxId`` if the identity is registered, or `nil` if it has never been seen on the network.
	public func inboxIdFromIdentity(identity: PublicIdentity) async throws
		-> InboxId?
	{
		try await ffiClient.findInboxId(identifier: identity.ffiPrivate)
	}

	/// Manually trigger a device sync request to sync records from another active device on this account.
	public func sendSyncRequest(
		opts: ArchiveOptions = ArchiveOptions(),
		serverUrl: String? = nil
	) async throws {
		let resolvedUrl = serverUrl ?? environment.getHistorySyncUrl()
		try await ffiClient.sendSyncRequest(options: opts.toFfi(), serverUrl: resolvedUrl)
	}

	/// Signs a message using this installation's private MLS key.
	///
	/// - Parameter message: The plaintext message to sign.
	/// - Returns: The raw signature bytes.
	public func signWithInstallationKey(message: String) throws -> Data {
		try ffiClient.signWithInstallationKey(text: message)
	}

	/// Verifies that a signature was produced by this installation's key.
	///
	/// - Parameters:
	///   - message: The original plaintext message that was signed.
	///   - signature: The raw signature bytes to verify.
	/// - Returns: `true` if the signature is valid, `false` otherwise.
	public func verifySignature(message: String, signature: Data) throws -> Bool {
		do {
			try ffiClient.verifySignedWithInstallationKey(
				signatureText: message, signatureBytes: signature
			)
			return true
		} catch {
			return false
		}
	}

	/// Verifies that a signature was produced by a specific installation's public key.
	///
	/// - Parameters:
	///   - message: The original plaintext message that was signed.
	///   - signature: The raw signature bytes to verify.
	///   - installationId: The hex-encoded installation ID whose public key should be used for verification.
	/// - Returns: `true` if the signature is valid for the given installation, `false` otherwise.
	public func verifySignatureWithInstallationId(
		message: String, signature: Data, installationId: String
	) throws -> Bool {
		do {
			try ffiClient.verifySignedWithPublicKey(
				signatureText: message, signatureBytes: signature,
				publicKey: installationId.hexToData
			)
			return true
		} catch {
			return false
		}
	}

	/// Returns the full identity state for this client's inbox, including all linked accounts and installations.
	///
	/// - Parameter refreshFromNetwork: When `true`, fetches the latest state from the network
	///   instead of returning the locally cached version.
	/// - Returns: An ``InboxState`` describing the inbox's current installations and linked identities.
	public func inboxState(refreshFromNetwork: Bool) async throws -> InboxState {
		try await InboxState(
			ffiInboxState: ffiClient.inboxState(
				refreshFromNetwork: refreshFromNetwork
			)
		)
	}

	/// Fetches the identity state for a list of inbox IDs.
	///
	/// - Parameters:
	///   - refreshFromNetwork: When `true`, fetches the latest state from the network.
	///   - inboxIds: The inbox IDs to query.
	/// - Returns: An array of ``InboxState`` values for the requested inboxes.
	public func inboxStatesForInboxIds(
		refreshFromNetwork: Bool, inboxIds: [InboxId]
	) async throws -> [InboxState] {
		try await ffiClient.addressesFromInboxId(
			refreshFromNetwork: refreshFromNetwork, inboxIds: inboxIds
		).map { InboxState(ffiInboxState: $0) }
	}

	/// Manually send a sync archive to the sync group.
	/// The pin will be later used as a reference when importing.
	public func sendSyncArchive(
		opts: ArchiveOptions = ArchiveOptions(),
		serverUrl: String? = nil,
		pin: String
	) async throws {
		let resolvedUrl = serverUrl ?? environment.getHistorySyncUrl()
		try await ffiClient.sendSyncArchive(options: opts.toFfi(), serverUrl: resolvedUrl, pin: pin)
	}

	/// Manually process a sync archive that matches the pin given.
	/// If no pin is given, then it will process the last archive sent.
	public func processSyncArchive(archivePin: String? = nil) async throws {
		try await ffiClient.processSyncArchive(archivePin: archivePin)
	}

	/// List the archives available for import in the sync group.
	/// You may need to manually sync the sync group before calling
	/// this function to see recently uploaded archives.
	public func listAvailableArchives(daysCutoff: Int64) throws
		-> [AvailableArchive]
	{
		try ffiClient.listAvailableArchives(daysCutoff: daysCutoff)
			.map { AvailableArchive($0) }
	}

	/// Manually sync all device sync groups.
	public func syncAllDeviceSyncGroups() async throws -> GroupSyncSummary {
		try await GroupSyncSummary(
			ffiGroupSyncSummary: ffiClient.syncAllDeviceSyncGroups()
		)
	}

	/// Creates an encrypted archive of the local database at the specified path.
	///
	/// - Parameters:
	///   - path: The file-system path where the archive will be written.
	///   - encryptionKey: A symmetric key used to encrypt the archive.
	///   - opts: Options controlling which data to include in the archive.
	public func createArchive(
		path: String,
		encryptionKey: Data,
		opts: ArchiveOptions = ArchiveOptions()
	) async throws {
		try await ffiClient.createArchive(path: path, opts: opts.toFfi(), key: encryptionKey)
	}

	/// Imports an encrypted archive into the local database, restoring conversations and messages.
	///
	/// - Parameters:
	///   - path: The file-system path to the archive file.
	///   - encryptionKey: The symmetric key that was used to encrypt the archive.
	public func importArchive(path: String, encryptionKey: Data) async throws {
		try await ffiClient.importArchive(path: path, key: encryptionKey)
	}

	/// Reads metadata from an encrypted archive without fully importing it.
	///
	/// - Parameters:
	///   - path: The file-system path to the archive file.
	///   - encryptionKey: The symmetric key that was used to encrypt the archive.
	/// - Returns: An ``ArchiveMetadata`` value describing the archive's contents and creation date.
	public func archiveMetadata(path: String, encryptionKey: Data) async throws
		-> ArchiveMetadata
	{
		let ffiMetadata = try await ffiClient.archiveMetadata(
			path: path, key: encryptionKey
		)
		return ArchiveMetadata(ffiMetadata)
	}

	/// Applies a pre-built signature request to the network for manual signature management.
	///
	/// - Note: Deprecated. Use ``addAccount(newAccount:allowReassignInboxId:)``,
	///   ``removeAccount(recoveryAccount:identityToRemove:)``, or
	///   ``revokeInstallations(signingKey:installationIds:)`` instead,
	///   which manage the signature flow automatically.
	@available(
		*,
		deprecated,
		message: """
		This function is delicate and should be used with caution.
		Should only be used if trying to manage the signature flow independently;
		otherwise use `addAccount()`, `removeAccount()`, or `revoke()` instead.
		"""
	)
	public func ffiApplySignatureRequest(signatureRequest: SignatureRequest)
		async throws
	{
		try await ffiClient.applySignatureRequest(
			signatureRequest: signatureRequest.ffiSignatureRequest
		)
	}

	/// Creates a revocation signature request for specific installations for manual signature management.
	///
	/// - Note: Deprecated. Use ``revokeInstallations(signingKey:installationIds:)`` instead,
	///   which manages the signature flow automatically.
	@available(
		*,
		deprecated,
		message: """
		This function is delicate and should be used with caution.
		Should only be used if trying to manage the signature flow independently;
		otherwise use `revokeInstallations()` instead.
		"""
	)
	public func ffiRevokeInstallations(ids: [Data]) async throws
		-> SignatureRequest
	{
		let ffiSigReq = try await ffiClient.revokeInstallations(
			installationIds: ids
		)
		return SignatureRequest(ffiSignatureRequest: ffiSigReq)
	}

	/// Creates a signature request to revoke all other installations for manual signature management.
	///
	/// - Note: Deprecated. Use ``revokeAllOtherInstallations(signingKey:)`` instead,
	///   which manages the signature flow automatically.
	@available(
		*,
		deprecated,
		message: """
		This function is delicate and should be used with caution.
		Should only be used if trying to manage the signature flow independently;
		otherwise use `revokeAllOtherInstallations()` instead.
		"""
	)
	public func ffiRevokeAllOtherInstallations() async throws
		-> SignatureRequest?
	{
		try await ffiClient
			.revokeAllOtherInstallationsSignatureRequest()
			.map(SignatureRequest.init(ffiSignatureRequest:))
	}

	/// Creates a signature request to remove an identity for manual signature management.
	///
	/// - Note: Deprecated. Use ``removeAccount(recoveryAccount:identityToRemove:)`` instead,
	///   which manages the signature flow automatically.
	@available(
		*,
		deprecated,
		message: """
		This function is delicate and should be used with caution.
		Should only be used if trying to manage the signature flow independently;
		otherwise use `removeIdentity()` instead.
		"""
	)
	public func ffiRevokeIdentity(identityToRemove: PublicIdentity) async throws
		-> SignatureRequest
	{
		let ffiSigReq = try await ffiClient.revokeIdentity(
			identifier: identityToRemove.ffiPrivate
		)
		return SignatureRequest(ffiSignatureRequest: ffiSigReq)
	}

	/// Creates a signature request to add an identity for manual signature management.
	///
	/// - Note: Deprecated. Use ``addAccount(newAccount:allowReassignInboxId:)`` instead,
	///   which manages the signature flow automatically.
	@available(
		*,
		deprecated,
		message: """
		This function is delicate and should be used with caution.
		Should only be used if trying to manage the create and register flow independently;
		otherwise use `addIdentity()` instead.
		"""
	)
	public func ffiAddIdentity(
		identityToAdd: PublicIdentity, allowReassignInboxId: Bool = false
	) async throws
		-> SignatureRequest
	{
		let inboxId: InboxId? =
			await !allowReassignInboxId
				? try inboxIdFromIdentity(
					identity: PublicIdentity(
						kind: identityToAdd.kind,
						identifier: identityToAdd.identifier
					)
				) : nil

		if allowReassignInboxId || (inboxId?.isEmpty ?? true) {
			let ffiSigReq = try await ffiClient.addIdentity(
				newIdentity: identityToAdd.ffiPrivate
			)
			return SignatureRequest(ffiSignatureRequest: ffiSigReq)
		} else {
			throw ClientError.creationError(
				"This wallet is already associated with inbox \(inboxId ?? "Unknown")"
			)
		}
	}

	/// Returns the pending signature request for this client, if any, for manual signature management.
	///
	/// - Note: Deprecated. Use ``create(account:options:)`` instead, which handles the
	///   signature flow internally.
	@available(
		*,
		deprecated,
		message: """
		This function is delicate and should be used with caution.
		Should only be used if trying to manage the signature flow independently;
		otherwise use `create()` instead.
		"""
	)
	public func ffiSignatureRequest() -> SignatureRequest? {
		guard let ffiReq = ffiClient.signatureRequest() else {
			return nil
		}
		return SignatureRequest(ffiSignatureRequest: ffiReq)
	}

	/// Registers the client's identity on the network using a pre-built signature request.
	///
	/// - Note: Deprecated. Use ``create(account:options:)`` instead, which handles identity
	///   registration automatically as part of client creation.
	@available(
		*,
		deprecated,
		message: """
		This function is delicate and should be used with caution.
		Should only be used if trying to manage the create and register flow independently;
		otherwise use `create()` instead.
		"""
	)
	public func ffiRegisterIdentity(signatureRequest: SignatureRequest)
		async throws
	{
		try await ffiClient.registerIdentity(
			signatureRequest: signatureRequest.ffiSignatureRequest
		)
	}
}

public extension Client {
	/// Log level for XMTP logging
	enum LogLevel {
		/// Error level logs only
		case error
		/// Warning level and above
		case warn
		/// Info level and above
		case info
		/// Debug level and above
		case debug

		fileprivate var ffiLogLevel: FfiLogLevel {
			switch self {
			case .error: .error
			case .warn: .warn
			case .info: .info
			case .debug: .debug
			}
		}
	}

	/// Activates persistent logging for LibXMTP
	/// - Parameters:
	///   - logLevel: The level of logging to capture
	///   - rotationSchedule: When log files should rotate
	///   - maxFiles: Maximum number of log files to keep
	///   - customLogDirectory: Optional custom directory path for logs
	///   - processType: The type of process (main app or notification extension)
	static func activatePersistentLibXMTPLogWriter(
		logLevel: LogLevel,
		rotationSchedule: FfiLogRotation,
		maxFiles: Int,
		customLogDirectory: URL? = nil,
		processType: FfiProcessType = .main
	) {
		let fileManager = FileManager.default
		let logDirectory =
			customLogDirectory
				?? URL.documentsDirectory.appendingPathComponent("xmtp_logs")

		// Check if directory exists and is writable before proceeding
		if !fileManager.fileExists(atPath: logDirectory.path) {
			do {
				try fileManager.createDirectory(
					at: logDirectory,
					withIntermediateDirectories: true,
					attributes: nil
				)
			} catch {
				os_log(
					"Failed to create log directory: %{public}@",
					log: OSLog.default, type: .error, error.localizedDescription
				)
				return
			}
		}

		// Verify write permissions by attempting to create a test file
		let testFilePath = logDirectory.appendingPathComponent("write_test.tmp")
		if !fileManager.createFile(
			atPath: testFilePath.path, contents: Data("test".utf8)
		) {
			os_log(
				"Directory exists but is not writable: %{public}@",
				log: OSLog.default, type: .error, logDirectory.path
			)
			return
		}

		// Clean up test file
		do {
			try fileManager.removeItem(at: testFilePath)
		} catch {
			// If we can't remove the test file, log but continue
			os_log(
				"Could not remove test file: %{public}@", log: OSLog.default,
				type: .error, error.localizedDescription
			)
		}

		// Install a signal handler to prevent app crashes on panics
		signal(SIGABRT) { _ in
			os_log(
				"Caught SIGABRT from Rust panic in logging", log: OSLog.default,
				type: .error
			)
			// Try to safely deactivate the logger
			do {
				try exitDebugWriter()
			} catch {
				// Already in a bad state, just log
				os_log(
					"Failed to deactivate logger after panic",
					log: OSLog.default, type: .error
				)
			}
		}

		do {
			try enterDebugWriter(
				directory: logDirectory.path,
				logLevel: logLevel.ffiLogLevel,
				rotation: rotationSchedule,
				maxFiles: UInt32(maxFiles),
				processType: processType
			)
		} catch {
			os_log(
				"Failed to activate persistent log writer: %{public}@",
				log: OSLog.default, type: .error, error.localizedDescription
			)
		}
	}

	/// Deactivates the persistent log writer
	static func deactivatePersistentLibXMTPLogWriter() {
		do {
			try exitDebugWriter()
		} catch {
			os_log(
				"Failed to deactivate persistent log writer: %{public}@",
				log: OSLog.default, type: .error, error.localizedDescription
			)
		}
	}

	/// Returns paths to all XMTP log files
	/// - Parameter customLogDirectory: Optional custom directory path for logs
	/// - Returns: Array of file paths to log files
	static func getXMTPLogFilePaths(customLogDirectory: URL? = nil)
		-> [String]
	{
		let fileManager = FileManager.default
		let logDirectory =
			customLogDirectory
				?? URL.documentsDirectory.appendingPathComponent("xmtp_logs")

		if !fileManager.fileExists(atPath: logDirectory.path) {
			return []
		}

		do {
			let fileURLs = try fileManager.contentsOfDirectory(
				at: logDirectory,
				includingPropertiesForKeys: [.isRegularFileKey],
				options: []
			)

			return fileURLs.compactMap { url in
				do {
					let resourceValues = try url.resourceValues(forKeys: [
						.isRegularFileKey,
					])
					return resourceValues.isRegularFile == true ? url.path : nil
				} catch {
					return nil
				}
			}
		} catch {
			return []
		}
	}

	/// Clears all XMTP log files
	/// - Parameter customLogDirectory: Optional custom directory path for logs
	/// - Returns: Number of files deleted
	@discardableResult
	static func clearXMTPLogs(customLogDirectory: URL? = nil) -> Int {
		let fileManager = FileManager.default
		let logDirectory =
			customLogDirectory
				?? URL.documentsDirectory.appendingPathComponent("xmtp_logs")

		if !fileManager.fileExists(atPath: logDirectory.path) {
			return 0
		}

		do {
			deactivatePersistentLibXMTPLogWriter()
		} catch {
			// Log writer might not be active, continue with deletion
		}

		var deletedCount = 0

		do {
			let fileURLs = try fileManager.contentsOfDirectory(
				at: logDirectory,
				includingPropertiesForKeys: [.isRegularFileKey],
				options: []
			)

			for fileURL in fileURLs {
				do {
					let resourceValues = try fileURL.resourceValues(forKeys: [
						.isRegularFileKey,
					])
					if resourceValues.isRegularFile == true {
						try fileManager.removeItem(at: fileURL)
						deletedCount += 1
					}
				} catch {
					// Continue with other files if one deletion fails
				}
			}
		} catch {
			// Return current count if directory listing fails
		}

		return deletedCount
	}

	private static func getNumberOfFilesInDirectory(directory: String?) -> Int {
		guard let directory else {
			XMTPLogger.database.error("Directory is nil")
			return 0
		}

		let fileManager = FileManager.default
		let directoryURL = URL(fileURLWithPath: directory, isDirectory: true)

		// Check if directory exists
		if !fileManager.fileExists(atPath: directory) {
			XMTPLogger.database.error("Directory does not exist: \(directory)")
			return 0
		}

		do {
			let contents = try fileManager.contentsOfDirectory(
				at: directoryURL,
				includingPropertiesForKeys: [.isRegularFileKey],
				options: []
			)

			// Log the contents found
			XMTPLogger.database.debug(
				"Found \(contents.count) items in directory"
			)

			// Count only regular files, not directories
			var fileCount = 0
			for url in contents {
				do {
					let resourceValues = try url.resourceValues(forKeys: [
						.isRegularFileKey,
					])
					if resourceValues.isRegularFile == true {
						fileCount += 1
						XMTPLogger.database.debug(
							"Regular file found: \(url.lastPathComponent)"
						)
					} else {
						XMTPLogger.database.debug(
							"Non-regular file found: \(url.lastPathComponent)"
						)
					}
				} catch {
					XMTPLogger.database.error(
						"Error checking file type: \(error.localizedDescription)"
					)
				}
			}

			return fileCount
		} catch {
			XMTPLogger.database.error(
				"Error reading directory: \(error.localizedDescription)"
			)
			return 0
		}
	}
}
