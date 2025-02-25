import Foundation
import LibXMTP

public typealias PreEventCallback = () async throws -> Void

public enum ClientError: Error, CustomStringConvertible, LocalizedError {
	case creationError(String)
	case missingInboxId

	public var description: String {
		switch self {
		case .creationError(let err):
			return "ClientError.creationError: \(err)"
		case .missingInboxId:
			return "ClientError.missingInboxId"
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

		/// /// Optional: Specify self-reported version e.g. XMTPInbox/v1.0.0.
		public var appVersion: String?

		public init(
			env: XMTPEnvironment = .dev, isSecure: Bool = true,
			appVersion: String? = nil
		) {
			self.env = env
			self.isSecure = isSecure
			self.appVersion = appVersion
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

public final class Client {
	public let address: String
	public let inboxID: String
	public let libXMTPVersion: String = getVersionInfo()
	public let dbPath: String
	public let installationID: String
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
		accountAddress: String,
		options: ClientOptions,
		signingKey: SigningKey?,
		inboxId: String,
		apiClient: XmtpApiClient? = nil
	) async throws -> Client {
		let (libxmtpClient, dbPath) = try await initFFiClient(
			accountAddress: accountAddress.lowercased(),
			options: options,
			inboxId: inboxId
		)

		let client = try Client(
			address: accountAddress.lowercased(),
			ffiClient: libxmtpClient,
			dbPath: dbPath,
			installationID: libxmtpClient.installationId().toHex,
			inboxID: libxmtpClient.inboxId(),
			environment: options.api.env
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
		let accountAddress = account.address.lowercased()
		let inboxId = try await getOrCreateInboxId(
			api: options.api, address: accountAddress)

		return try await initializeClient(
			accountAddress: accountAddress,
			options: options,
			signingKey: account,
			inboxId: inboxId
		)
	}

	public static func build(
		address: String, options: ClientOptions, inboxId: String? = nil
	)
		async throws -> Client
	{
		let accountAddress = address.lowercased()
		let resolvedInboxId: String
		if let existingInboxId = inboxId {
			resolvedInboxId = existingInboxId
		} else {
			resolvedInboxId = try await getOrCreateInboxId(
				api: options.api, address: accountAddress)
		}

		return try await initializeClient(
			accountAddress: accountAddress,
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
		address: String, clientOptions: ClientOptions
	) async throws -> Client {
		let accountAddress = address.lowercased()
		let recoveredInboxId = try await getOrCreateInboxId(
			api: clientOptions.api, address: accountAddress)

		let (ffiClient, dbPath) = try await initFFiClient(
			accountAddress: accountAddress,
			options: clientOptions,
			inboxId: recoveredInboxId
		)

		return try Client(
			address: accountAddress,
			ffiClient: ffiClient,
			dbPath: dbPath,
			installationID: ffiClient.installationId().toHex,
			inboxID: ffiClient.inboxId(),
			environment: clientOptions.api.env
		)
	}

	private static func initFFiClient(
		accountAddress: String,
		options: ClientOptions,
		inboxId: String
	) async throws -> (FfiXmtpClient, String) {
		let address = accountAddress.lowercased()

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
			accountAddress: address,
			nonce: 0,
			legacySignedPrivateKeyProto: nil,
			historySyncUrl: options.historySyncUrl
		)

		return (ffiClient, dbURL)
	}

	private static func handleSignature(
		for signatureRequest: FfiSignatureRequest,
		signingKey: SigningKey
	) async throws {
		if signingKey.type == .SCW {
			guard let chainId = signingKey.chainId else {
				throw ClientError.creationError(
					"Chain id must be present to sign Smart Contract Wallet")
			}
			let signedData = try await signingKey.signSCW(
				message: signatureRequest.signatureText())
			try await signatureRequest.addScwSignature(
				signatureBytes: signedData,
				address: signingKey.address.lowercased(),
				chainId: UInt64(chainId),
				blockNumber: signingKey.blockNumber.flatMap {
					$0 >= 0 ? UInt64($0) : nil
				}
			)
		} else {
			let signedData = try await signingKey.sign(
				message: signatureRequest.signatureText())
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
		api: ClientOptions.Api, address: String
	) async throws -> String {
		var inboxId: String
		do {
			inboxId =
				try await getInboxIdForAddress(
					api: connectToApiBackend(api: api),
					accountAddress: address.lowercased()
				)
				?? generateInboxId(
					accountAddress: address.lowercased(), nonce: 0)
		} catch {
			inboxId = try generateInboxId(
				accountAddress: address.lowercased(), nonce: 0)
		}
		return inboxId
	}

	private static func prepareClient(
		api: ClientOptions.Api,
		address: String = "0x0000000000000000000000000000000000000000"
	) async throws -> FfiXmtpClient {
		let inboxId = try await getOrCreateInboxId(api: api, address: address)
		return try await LibXMTP.createClient(
			api: connectToApiBackend(api: api),
			db: nil,
			encryptionKey: nil,
			inboxId: inboxId,
			accountAddress: address,
			nonce: 0,
			legacySignedPrivateKeyProto: nil,
			historySyncUrl: nil
		)
	}

	public static func canMessage(
		accountAddresses: [String],
		api: ClientOptions.Api
	) async throws -> [String: Bool] {
		let ffiClient = try await prepareClient(api: api)
		return try await ffiClient.canMessage(
			accountAddresses: accountAddresses)
	}

	public static func inboxStatesForInboxIds(
		inboxIds: [String],
		api: ClientOptions.Api
	) async throws -> [InboxState] {
		let ffiClient = try await prepareClient(api: api)
		let result = try await ffiClient.addressesFromInboxId(
			refreshFromNetwork: true, inboxIds: inboxIds)
		return result.map { InboxState(ffiInboxState: $0) }
	}

	init(
		address: String, ffiClient: LibXMTP.FfiXmtpClient, dbPath: String,
		installationID: String, inboxID: String, environment: XMTPEnvironment
	) throws {
		self.address = address
		self.ffiClient = ffiClient
		self.dbPath = dbPath
		self.installationID = installationID
		self.inboxID = inboxID
		self.environment = environment
	}

	@available(
		*, deprecated,
		message:
			"This function is delicate and should be used with caution. Adding a wallet already associated with an inboxId will cause the wallet to loose access to that inbox. See: inboxIdFromAddress(address)"
	)
	public func addAccount(
		newAccount: SigningKey, allowReassignInboxId: Bool = false
	)
		async throws
	{
		let inboxId: String? =
			allowReassignInboxId
			? nil : try await inboxIdFromAddress(address: newAccount.address)

		if allowReassignInboxId || (inboxId?.isEmpty ?? true) {
			let signatureRequest = try await ffiAddWallet(
				addressToAdd: newAccount.address.lowercased())

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
		recoveryAccount: SigningKey, addressToRemove: String
	) async throws {
		let signatureRequest = try await ffiRevokeWallet(
			addressToRemove: addressToRemove.lowercased())
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

	public func canMessage(address: String) async throws -> Bool {
		let canMessage = try await ffiClient.canMessage(accountAddresses: [
			address
		])
		return canMessage[address.lowercased()] ?? false
	}

	public func canMessage(addresses: [String]) async throws -> [String: Bool] {
		return try await ffiClient.canMessage(accountAddresses: addresses)
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

	public func inboxIdFromAddress(address: String) async throws -> String? {
		return try await ffiClient.findInboxId(address: address.lowercased())
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
		refreshFromNetwork: Bool, inboxIds: [String]
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
			otherwise use `removeWallet()` instead.
			"""
	)
	public func ffiRevokeWallet(addressToRemove: String) async throws
		-> SignatureRequest
	{
		let ffiSigReq = try await ffiClient.revokeWallet(
			walletAddress: addressToRemove.lowercased())
		return SignatureRequest(ffiSignatureRequest: ffiSigReq)
	}

	@available(
		*,
		deprecated,
		message: """
			This function is delicate and should be used with caution. 
			Should only be used if trying to manage the create and register flow independently; 
			otherwise use `addWallet()` instead.
			"""
	)
	public func ffiAddWallet(addressToAdd: String) async throws
		-> SignatureRequest
	{
		let ffiSigReq = try await ffiClient.addWallet(
			newWalletAddress: addressToAdd.lowercased())
		return SignatureRequest(ffiSignatureRequest: ffiSigReq)
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
