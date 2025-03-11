import Foundation
import LibXMTP

public typealias PreEventCallback = () async throws -> Void

public enum ClientError: Error, CustomStringConvertible, LocalizedError {
	case creationError(String)
	case missingInboxId
	case invalidInboxId(String)

	public var description: String {
		switch self {
		case .creationError(let err):
			return "ClientError.creationError: \(err)"
		case .missingInboxId:
			return "ClientError.missingInboxId"
		case .invalidInboxId(let inboxId):
			return
				"Invalid inboxId: \(inboxId). Inbox IDs cannot start with '0x'."
		}
	}

	public var errorDescription: String? {
		return description
	}
}

/// Specify configuration options for creating a ``Client``.
public struct ClientOptions {
	// Specify network options
	public struct Api {
		/// Specify which XMTP network to connect to. Defaults to ``.dev``
		public var env: XMTPEnvironment = .dev

		/// Specify whether the API client should use TLS security. In general this should only be false when using the `.local` environment.
		public var isSecure: Bool = true

		public init(
			env: XMTPEnvironment = .dev, isSecure: Bool = true
		) {
			self.env = env
			self.isSecure = isSecure
		}
	}

	public var api = Api()
	public var codecs: [any ContentCodec] = []

	/// `preAuthenticateToInboxCallback` will be called immediately before an Auth Inbox signature is requested from the user
	public var preAuthenticateToInboxCallback: PreEventCallback?

	public var dbEncryptionKey: Data
	public var dbDirectory: String?
	public var historySyncUrl: String?

	public init(
		api: Api = Api(),
		codecs: [any ContentCodec] = [],
		preAuthenticateToInboxCallback: PreEventCallback? = nil,
		dbEncryptionKey: Data,
		dbDirectory: String? = nil,
		historySyncUrl: String? = nil,
		useDefaultHistorySyncUrl: Bool = true
	) {
		self.api = api
		self.codecs = codecs
		self.preAuthenticateToInboxCallback = preAuthenticateToInboxCallback
		self.dbEncryptionKey = dbEncryptionKey
		self.dbDirectory = dbDirectory
		if useDefaultHistorySyncUrl && historySyncUrl == nil {
			switch api.env {
			case .production:
				self.historySyncUrl =
					"https://message-history.production.ephemera.network/"
			case .local:
				self.historySyncUrl = "http://localhost:5558"
			default:
				self.historySyncUrl =
					"https://message-history.dev.ephemera.network/"
			}
		} else {
			self.historySyncUrl = historySyncUrl
		}
	}
}

actor ApiClientCache {
	private var apiClientCache: [String: XmtpApiClient] = [:]

	func getClient(forKey key: String) -> XmtpApiClient? {
		return apiClientCache[key]
	}

	func setClient(_ client: XmtpApiClient, forKey key: String) {
		apiClientCache[key] = client
	}
}

public typealias InboxId = String

public final class Client {
	public let inboxID: InboxId
	public let libXMTPVersion: String = getVersionInfo()
	public let dbPath: String
	public let installationID: String
	public let publicIdentity: PublicIdentity
	public let environment: XMTPEnvironment
	private let ffiClient: LibXMTP.FfiXmtpClient
	private static let apiCache = ApiClientCache()

	public lazy var conversations: Conversations = .init(
		client: self, ffiConversations: ffiClient.conversations(),
		ffiClient: ffiClient)
	public lazy var preferences: PrivatePreferences = .init(
		client: self, ffiClient: ffiClient)

	static var codecRegistry = CodecRegistry()

	public static func register(codec: any ContentCodec) {
		codecRegistry.register(codec: codec)
	}

	static func initializeClient(
		publicIdentity: PublicIdentity,
		options: ClientOptions,
		signingKey: SigningKey?,
		inboxId: InboxId,
		apiClient: XmtpApiClient? = nil
	) async throws -> Client {
		let (libxmtpClient, dbPath) = try await initFFiClient(
			accountIdentifier: publicIdentity,
			options: options,
			inboxId: inboxId
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
			if let signingKey = signingKey {
				do {
					try await handleSignature(
						for: signatureRequest, signingKey: signingKey)
					try await client.ffiClient.registerIdentity(
						signatureRequest: signatureRequest)
				} catch {
					throw ClientError.creationError(
						"Failed to sign the message: \(error.localizedDescription)"
					)
				}
			} else {
				throw ClientError.creationError(
					"No v3 keys found, you must pass a SigningKey in order to enable alpha MLS features"
				)
			}
		}

		// Register codecs
		for codec in options.codecs {
			register(codec: codec)
		}

		return client
	}

	public static func create(
		account: SigningKey, options: ClientOptions
	)
		async throws -> Client
	{
		let identity = account.identity
		let inboxId = try await getOrCreateInboxId(
			api: options.api, publicIdentity: identity)

		return try await initializeClient(
			publicIdentity: identity,
			options: options,
			signingKey: account,
			inboxId: inboxId
		)
	}

	public static func build(
		publicIdentity: PublicIdentity, options: ClientOptions,
		inboxId: InboxId? = nil
	)
		async throws -> Client
	{
		let resolvedInboxId: String
		if let existingInboxId = inboxId {
			resolvedInboxId = existingInboxId
		} else {
			resolvedInboxId = try await getOrCreateInboxId(
				api: options.api, publicIdentity: publicIdentity)
		}

		return try await initializeClient(
			publicIdentity: publicIdentity,
			options: options,
			signingKey: nil,
			inboxId: resolvedInboxId
		)
	}

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
			api: clientOptions.api, publicIdentity: identity)

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
		inboxId: InboxId
	) async throws -> (FfiXmtpClient, String) {
		let mlsDbDirectory = options.dbDirectory
		var directoryURL: URL
		if let mlsDbDirectory = mlsDbDirectory {
			let fileManager = FileManager.default
			directoryURL = URL(
				fileURLWithPath: mlsDbDirectory, isDirectory: true)
			// Check if the directory exists, if not, create it
			if !fileManager.fileExists(atPath: directoryURL.path) {
				do {
					try fileManager.createDirectory(
						at: directoryURL, withIntermediateDirectories: true,
						attributes: nil)
				} catch {
					throw ClientError.creationError(
						"Failed db directory \(mlsDbDirectory)")
				}
			}
		} else {
			directoryURL = URL.documentsDirectory
		}

		let alias = "xmtp-\(options.api.env.rawValue)-\(inboxId).db3"
		let dbURL = directoryURL.appendingPathComponent(alias).path

		let ffiClient = try await LibXMTP.createClient(
			api: connectToApiBackend(api: options.api),
			db: dbURL,
			encryptionKey: options.dbEncryptionKey,
			inboxId: inboxId,
			accountIdentifier: accountIdentifier.ffiPrivate,
			nonce: 0,
			legacySignedPrivateKeyProto: nil,
			historySyncUrl: options.historySyncUrl
		)

		return (ffiClient, dbURL)
	}

	private static func handleSignature(
		for signatureRequest: FfiSignatureRequest, signingKey: SigningKey
	) async throws {
		let signedData = try await signingKey.sign(
			signatureRequest.signatureText())

		switch signingKey.type {
		case .SCW:
			guard let chainId = signingKey.chainId else {
				throw ClientError.creationError(
					"Chain id must be present to sign Smart Contract Wallet")
			}
			try await signatureRequest.addScwSignature(
				signatureBytes: signedData.rawData,
				address: signingKey.identity.identifier,
				chainId: UInt64(chainId),
				blockNumber: signingKey.blockNumber.map { UInt64($0) }
			)

		case .EOA:
			try await signatureRequest.addEcdsaSignature(
				signatureBytes: signedData.rawData)
		}
	}

	public static func connectToApiBackend(api: ClientOptions.Api) async throws
		-> XmtpApiClient
	{
		let cacheKey = api.env.url

		if let cachedClient = await apiCache.getClient(forKey: cacheKey) {
			return cachedClient
		}

		let apiClient = try await connectToBackend(
			host: api.env.url, isSecure: api.isSecure)
		await apiCache.setClient(apiClient, forKey: cacheKey)
		return apiClient
	}

	public static func getOrCreateInboxId(
		api: ClientOptions.Api, publicIdentity: PublicIdentity
	) async throws -> InboxId {
		var inboxId: String
		do {
			inboxId =
				try await getInboxIdForIdentifier(
					api: connectToApiBackend(api: api),
					accountIdentifier: publicIdentity.ffiPrivate)
				?? generateInboxId(
					accountIdentifier: publicIdentity.ffiPrivate, nonce: 0)
		} catch {
			inboxId = try generateInboxId(
				accountIdentifier: publicIdentity.ffiPrivate, nonce: 0)
		}
		return inboxId
	}

	private static func prepareClient(
		api: ClientOptions.Api,
		identity: PublicIdentity = PublicIdentity(
			kind: .ethereum,
			identifier: "0x0000000000000000000000000000000000000000")
	) async throws -> FfiXmtpClient {
		let inboxId = try await getOrCreateInboxId(
			api: api, publicIdentity: identity)
		return try await LibXMTP.createClient(
			api: connectToApiBackend(api: api),
			db: nil,
			encryptionKey: nil,
			inboxId: inboxId,
			accountIdentifier: identity.ffiPrivate,
			nonce: 0,
			legacySignedPrivateKeyProto: nil,
			historySyncUrl: nil
		)
	}

	public static func canMessage(
		identities: [PublicIdentity], api: ClientOptions.Api
	) async throws -> [String: Bool] {
		let ffiClient = try await prepareClient(api: api)
		let ffiIdentifiers = identities.map { $0.ffiPrivate }
		let result = try await ffiClient.canMessage(
			accountIdentifiers: ffiIdentifiers)

		return Dictionary(
			uniqueKeysWithValues: result.map { ($0.key.identifier, $0.value) })
	}

	public static func inboxStatesForInboxIds(
		inboxIds: [InboxId],
		api: ClientOptions.Api
	) async throws -> [InboxState] {
		let ffiClient = try await prepareClient(api: api)
		let result = try await ffiClient.addressesFromInboxId(
			refreshFromNetwork: true, inboxIds: inboxIds)
		return result.map { InboxState(ffiInboxState: $0) }
	}

	init(
		ffiClient: LibXMTP.FfiXmtpClient, dbPath: String,
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
					signingKey: newAccount)
				try await ffiApplySignatureRequest(
					signatureRequest: signatureRequest)
			} catch {
				throw ClientError.creationError(
					"Failed to sign the message: \(error.localizedDescription)")
			}
		} else {
			throw ClientError.creationError(
				"This wallet is already associated with inbox \(inboxId ?? "Unknown")"
			)
		}
	}

	public func removeAccount(
		recoveryAccount: SigningKey, identityToRemove: PublicIdentity
	) async throws {
		let signatureRequest = try await ffiRevokeIdentity(
			identityToRemove: identityToRemove)
		do {
			try await Client.handleSignature(
				for: signatureRequest.ffiSignatureRequest,
				signingKey: recoveryAccount)
			try await ffiApplySignatureRequest(
				signatureRequest: signatureRequest)
		} catch {
			throw ClientError.creationError(
				"Failed to sign the message: \(error.localizedDescription)")
		}
	}

	public func revokeAllOtherInstallations(signingKey: SigningKey) async throws
	{
		let signatureRequest = try await ffiRevokeAllOtherInstallations()
		do {
			try await Client.handleSignature(
				for: signatureRequest.ffiSignatureRequest,
				signingKey: signingKey)
			try await ffiApplySignatureRequest(
				signatureRequest: signatureRequest)
		} catch {
			throw ClientError.creationError(
				"Failed to sign the message: \(error.localizedDescription)")
		}
	}

	public func revokeInstallations(
		signingKey: SigningKey, installationIds: [String]
	) async throws {
		let installations = installationIds.map { $0.hexToData }
		let signatureRequest = try await ffiRevokeInstallations(
			ids: installations)
		do {
			try await Client.handleSignature(
				for: signatureRequest.ffiSignatureRequest,
				signingKey: signingKey)
			try await ffiApplySignatureRequest(
				signatureRequest: signatureRequest)
		} catch {
			throw ClientError.creationError(
				"Failed to sign the message: \(error.localizedDescription)")
		}
	}

	public func canMessage(identities: PublicIdentity) async throws -> Bool {
		let canMessage = try await canMessage(identities: [
			identities
		])
		return canMessage[identities.identifier] ?? false
	}

	func canMessage(identities: [PublicIdentity]) async throws -> [String: Bool]
	{
		let ffiIdentifiers = identities.map { $0.ffiPrivate }
		let result = try await ffiClient.canMessage(
			accountIdentifiers: ffiIdentifiers)

		return Dictionary(
			uniqueKeysWithValues: result.map { ($0.key.identifier, $0.value) })
	}

	public func deleteLocalDatabase() throws {
		try dropLocalDatabaseConnection()
		let fm = FileManager.default
		try fm.removeItem(atPath: dbPath)
	}

	@available(
		*, deprecated,
		message:
			"This function is delicate and should be used with caution. App will error if database not properly reconnected. See: reconnectLocalDatabase()"
	)
	public func dropLocalDatabaseConnection() throws {
		try ffiClient.releaseDbConnection()
	}

	public func reconnectLocalDatabase() async throws {
		try await ffiClient.dbReconnect()
	}

	public func inboxIdFromIdentity(identity: PublicIdentity) async throws
		-> InboxId?
	{
		return try await ffiClient.findInboxId(identifier: identity.ffiPrivate)
	}

	public func signWithInstallationKey(message: String) throws -> Data {
		return try ffiClient.signWithInstallationKey(text: message)
	}

	public func verifySignature(message: String, signature: Data) throws -> Bool
	{
		do {
			try ffiClient.verifySignedWithInstallationKey(
				signatureText: message, signatureBytes: signature)
			return true
		} catch {
			return false
		}
	}

	public func verifySignatureWithInstallationId(
		message: String, signature: Data, installationId: String
	) throws -> Bool {
		do {
			try ffiClient.verifySignedWithPublicKey(
				signatureText: message, signatureBytes: signature,
				publicKey: installationId.hexToData)
			return true
		} catch {
			return false
		}
	}

	public func inboxState(refreshFromNetwork: Bool) async throws -> InboxState
	{
		return InboxState(
			ffiInboxState: try await ffiClient.inboxState(
				refreshFromNetwork: refreshFromNetwork))
	}

	public func inboxStatesForInboxIds(
		refreshFromNetwork: Bool, inboxIds: [InboxId]
	) async throws -> [InboxState] {
		return try await ffiClient.addressesFromInboxId(
			refreshFromNetwork: refreshFromNetwork, inboxIds: inboxIds
		).map { InboxState(ffiInboxState: $0) }
	}

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
			signatureRequest: signatureRequest.ffiSignatureRequest)
	}

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
			installationIds: ids)
		return SignatureRequest(ffiSignatureRequest: ffiSigReq)
	}

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
		-> SignatureRequest
	{
		let ffiSigReq = try await ffiClient.revokeAllOtherInstallations()
		return SignatureRequest(ffiSignatureRequest: ffiSigReq)
	}

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
			identifier: identityToRemove.ffiPrivate)
		return SignatureRequest(ffiSignatureRequest: ffiSigReq)
	}

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
				newIdentity: identityToAdd.ffiPrivate)
			return SignatureRequest(ffiSignatureRequest: ffiSigReq)
		} else {
			throw ClientError.creationError(
				"This wallet is already associated with inbox \(inboxId ?? "Unknown")"
			)
		}
	}

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
			signatureRequest: signatureRequest.ffiSignatureRequest)
	}
}
