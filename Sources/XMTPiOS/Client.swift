import Foundation
import LibXMTP
import os

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
            self.historySyncUrl = api.env.getHistorySyncUrl()
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
    
    public lazy var debugInformation: XMTPDebugInformation = .init(
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
				// add log messages here for logging 1) dbDirectory, 2) number of files in dbDirectory, 3) dbPath
				let dbPathDirectory = URL(fileURLWithPath: dbPath).deletingLastPathComponent().path
				XMTPLogger.database.error("custom dbDirectory: \(options.dbDirectory ?? "nil")")
				XMTPLogger.database.error("dbPath: \(dbPath)")
				XMTPLogger.database.error("dbPath Directory: \(dbPathDirectory)")
				XMTPLogger.database.error("Number of files in dbDirectory: \(getNumberOfFilesInDirectory(directory: dbPathDirectory))")
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
		var dbURL = directoryURL.appendingPathComponent(alias).path
		var fileExists = FileManager.default.fileExists(atPath: dbURL)

		if !fileExists {
			let legacyAlias = "xmtp-\(options.api.env.legacyRawValue)-\(inboxId).db3"
			let legacyDbURL = directoryURL.appendingPathComponent(legacyAlias).path
			let legacyFileExists = FileManager.default.fileExists(atPath: legacyDbURL)

			if legacyFileExists {
				dbURL = legacyDbURL
			}
		}
		let ffiClient = try await LibXMTP.createClient(
			api: connectToApiBackend(api: options.api),
			db: dbURL,
			encryptionKey: options.dbEncryptionKey,
			inboxId: inboxId,
			accountIdentifier: accountIdentifier.ffiPrivate,
			nonce: 0,
			legacySignedPrivateKeyProto: nil,
			deviceSyncServerUrl: options.historySyncUrl,
            deviceSyncMode: .enabled
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
			deviceSyncServerUrl: nil,
			deviceSyncMode: nil
		)
	}

	public static func canMessage(
		accountIdentities: [PublicIdentity], api: ClientOptions.Api
	) async throws -> [String: Bool] {
		let ffiClient = try await prepareClient(api: api)
		let ffiIdentifiers = accountIdentities.map { $0.ffiPrivate }
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
	
	public static func keyPackageStatusesForInstallationIds(
		installationIds: [String],
		api: ClientOptions.Api
	) async throws -> [String: FfiKeyPackageStatus] {
		let ffiClient = try await prepareClient(api: api)

		let byteArrays = installationIds.map { $0.hexToData }
		let result = try await ffiClient.getKeyPackageStatusesForInstallationIds(installationIds: byteArrays)
		var statusMap: [String: FfiKeyPackageStatus] = [:]
		for (keyBytes, status) in result {
			let keyHex = keyBytes.toHex
			statusMap[keyHex] = status
		}
		return statusMap
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

	public func canMessage(identity: PublicIdentity) async throws -> Bool {
		let canMessage = try await canMessage(identities: [
			identity
		])
		return canMessage[identity.identifier] ?? false
	}

	public func canMessage(identities: [PublicIdentity]) async throws -> [String: Bool]
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

extension Client {
	/// Log level for XMTP logging
	public enum LogLevel {
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
			case .error: return .error
			case .warn: return .warn
			case .info: return .info
			case .debug: return .debug
			}
		}
	}

	/// Activates persistent logging for LibXMTP
	/// - Parameters:
	///   - logLevel: The level of logging to capture
	///   - rotationSchedule: When log files should rotate
	///   - maxFiles: Maximum number of log files to keep
	///   - customLogDirectory: Optional custom directory path for logs
	public static func activatePersistentLibXMTPLogWriter(
		logLevel: LogLevel,
		rotationSchedule: FfiLogRotation,
		maxFiles: Int,
		customLogDirectory: URL? = nil
	) {
		
		let fileManager = FileManager.default
		let logDirectory = customLogDirectory ?? URL.documentsDirectory.appendingPathComponent("xmtp_logs")
		
		// Check if directory exists and is writable before proceeding
		if !fileManager.fileExists(atPath: logDirectory.path) {
			do {
				try fileManager.createDirectory(
					at: logDirectory,
					withIntermediateDirectories: true,
					attributes: nil
				)
			} catch {
				os_log("Failed to create log directory: %{public}@", log: OSLog.default, type: .error, error.localizedDescription)
				return
			}
		}
		
		// Verify write permissions by attempting to create a test file
		let testFilePath = logDirectory.appendingPathComponent("write_test.tmp")
		if !fileManager.createFile(atPath: testFilePath.path, contents: Data("test".utf8)) {
			os_log("Directory exists but is not writable: %{public}@", log: OSLog.default, type: .error, logDirectory.path)
			return
		}
		
		// Clean up test file
		do {
			try fileManager.removeItem(at: testFilePath)
		} catch {
			// If we can't remove the test file, log but continue
			os_log("Could not remove test file: %{public}@", log: OSLog.default, type: .error, error.localizedDescription)
		}
		
		// Install a signal handler to prevent app crashes on panics
		signal(SIGABRT) { _ in
			os_log("Caught SIGABRT from Rust panic in logging", log: OSLog.default, type: .error)
			// Try to safely deactivate the logger
			do {
				try exitDebugWriter()
			} catch {
				// Already in a bad state, just log
				os_log("Failed to deactivate logger after panic", log: OSLog.default, type: .error)
			}
		}
		
		do {
			try enterDebugWriter(
				directory: logDirectory.path,
				logLevel: logLevel.ffiLogLevel,
				rotation: rotationSchedule,
				maxFiles: UInt32(maxFiles)
			)
		} catch {
			os_log("Failed to activate persistent log writer: %{public}@", log: OSLog.default, type: .error, error.localizedDescription)
		}
	}
	
	/// Deactivates the persistent log writer
	public static func deactivatePersistentLibXMTPLogWriter() {
		do {
			try exitDebugWriter()
		} catch {
			os_log("Failed to deactivate persistent log writer: %{public}@", log: OSLog.default, type: .error, error.localizedDescription)
		}
	}
	
	/// Returns paths to all XMTP log files
	/// - Parameter customLogDirectory: Optional custom directory path for logs
	/// - Returns: Array of file paths to log files
	public static func getXMTPLogFilePaths(customLogDirectory: URL? = nil) -> [String] {
		let fileManager = FileManager.default
		let logDirectory = customLogDirectory ?? URL.documentsDirectory.appendingPathComponent("xmtp_logs")
		
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
					let resourceValues = try url.resourceValues(forKeys: [.isRegularFileKey])
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
	public static func clearXMTPLogs(customLogDirectory: URL? = nil) -> Int {
		let fileManager = FileManager.default
		let logDirectory = customLogDirectory ?? URL.documentsDirectory.appendingPathComponent("xmtp_logs")
		
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
					let resourceValues = try fileURL.resourceValues(forKeys: [.isRegularFileKey])
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
        guard let directory = directory else {
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
            XMTPLogger.database.debug("Found \(contents.count) items in directory")
            
            // Count only regular files, not directories
            var fileCount = 0
            for url in contents {
                do {
                    let resourceValues = try url.resourceValues(forKeys: [.isRegularFileKey])
                    if resourceValues.isRegularFile == true {
                        fileCount += 1
                        XMTPLogger.database.debug("Regular file found: \(url.lastPathComponent)")
                    } else {
                        XMTPLogger.database.debug("Non-regular file found: \(url.lastPathComponent)")
                    }
                } catch {
                    XMTPLogger.database.error("Error checking file type: \(error.localizedDescription)")
                }
            }
            
            return fileCount
        } catch {
            XMTPLogger.database.error("Error reading directory: \(error.localizedDescription)")
            return 0
        }
    }
}
