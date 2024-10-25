//
//  Client.swift
//
//
//  Created by Pat Nakajima on 11/22/22.
//

import Foundation
import LibXMTP
import web3

public typealias PreEventCallback = () async throws -> Void

public enum ClientError: Error, CustomStringConvertible, LocalizedError {
	case creationError(String)
	case noV3Client(String)
	case noV2Client(String)
	case missingInboxId

	public var description: String {
		switch self {
		case .creationError(let err):
			return "ClientError.creationError: \(err)"
		case .noV3Client(let err):
			return "ClientError.noV3Client: \(err)"
		case .noV2Client(let err):
			return "ClientError.noV2Client: \(err)"
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

		public init(env: XMTPEnvironment = .dev, isSecure: Bool = true, appVersion: String? = nil) {
			self.env = env
			self.isSecure = isSecure
			self.appVersion = appVersion
		}
	}

	public var api = Api()
	public var codecs: [any ContentCodec] = []

	/// `preEnableIdentityCallback` will be called immediately before an Enable Identity wallet signature is requested from the user.
	public var preEnableIdentityCallback: PreEventCallback?

	/// `preCreateIdentityCallback` will be called immediately before a Create Identity wallet signature is requested from the user.
	public var preCreateIdentityCallback: PreEventCallback?
    
    /// `preAuthenticateToInboxCallback` will be called immediately before an Auth Inbox signature is requested from the user
    public var preAuthenticateToInboxCallback: PreEventCallback?

	public var enableV3 = false
	public var dbEncryptionKey: Data?
	public var dbDirectory: String?
	public var historySyncUrl: String?

	public init(
		api: Api = Api(),
		codecs: [any ContentCodec] = [],
		preEnableIdentityCallback: PreEventCallback? = nil,
		preCreateIdentityCallback: PreEventCallback? = nil,
        preAuthenticateToInboxCallback: PreEventCallback? = nil,
		enableV3: Bool = false,
		encryptionKey: Data? = nil,
		dbDirectory: String? = nil,
		historySyncUrl: String? = nil
	) {
		self.api = api
		self.codecs = codecs
		self.preEnableIdentityCallback = preEnableIdentityCallback
		self.preCreateIdentityCallback = preCreateIdentityCallback
		self.preAuthenticateToInboxCallback = preAuthenticateToInboxCallback
		self.enableV3 = enableV3
		self.dbEncryptionKey = encryptionKey
		self.dbDirectory = dbDirectory
		if (historySyncUrl == nil) {
			switch api.env {
			case .production:
				self.historySyncUrl = "https://message-history.production.ephemera.network/"
			case .local:
				self.historySyncUrl = "http://0.0.0.0:5558"
			default:
				self.historySyncUrl = "https://message-history.dev.ephemera.network/"
			}
		} else {
			self.historySyncUrl = historySyncUrl
		}
	}
}

/// Client is the entrypoint into the XMTP SDK.
///
/// A client is created by calling ``create(account:options:)`` with a ``SigningKey`` that can create signatures on your behalf. The client will request a signature in two cases:
///
/// 1. To sign the newly generated key bundle. This happens only the very first time when a key bundle is not found in storage.
/// 2. To sign a random salt used to encrypt the key bundle in storage. This happens every time the client is started, including the very first time).
///
/// > Important: The client connects to the XMTP `dev` environment by default. Use ``ClientOptions`` to change this and other parameters of the network connection.
public final class Client {
	/// The wallet address of the ``SigningKey`` used to create this Client.
	public let address: String
	var privateKeyBundleV1: PrivateKeyBundleV1? = nil
	var apiClient: ApiClient? = nil
	public let v3Client: LibXMTP.FfiXmtpClient?
	public let libXMTPVersion: String = getVersionInfo()
	public let dbPath: String
	public let installationID: String
	public let inboxID: String
	public var hasV2Client: Bool = true


	/// Access ``Conversations`` for this Client.
	public lazy var conversations: Conversations = .init(client: self)

	/// Access ``Contacts`` for this Client.
	public lazy var contacts: Contacts = .init(client: self)

	/// The XMTP environment which specifies which network this Client is connected to.
	public lazy var environment: XMTPEnvironment = apiClient?.environment ?? .dev

	var codecRegistry = CodecRegistry()

	public func register(codec: any ContentCodec) {
		codecRegistry.register(codec: codec)
	}

	/// Creates a client.
	public static func create(account: SigningKey, options: ClientOptions? = nil) async throws -> Client {
		let options = options ?? ClientOptions()
		do {
			let client = try await LibXMTP.createV2Client(host: options.api.env.url, isSecure: options.api.env.isSecure)
			let apiClient = try GRPCApiClient(
				environment: options.api.env,
				secure: options.api.isSecure,
				rustClient: client
			)
			return try await create(account: account, apiClient: apiClient, options: options)
		} catch {
			let detailedErrorMessage: String
			if let nsError = error as NSError? {
				detailedErrorMessage = nsError.description
			} else {
				detailedErrorMessage = error.localizedDescription
			}
			throw ClientError.creationError(detailedErrorMessage)
		}
	}
	
	static func initializeClient(
		accountAddress: String,
		options: ClientOptions,
		signingKey: SigningKey?,
		inboxId: String
	) async throws -> Client {
		let (libxmtpClient, dbPath) = try await initV3Client(
			accountAddress: accountAddress,
			options: options,
			privateKeyBundleV1: nil,
			signingKey: signingKey,
			inboxId: inboxId
		)

		guard let v3Client = libxmtpClient else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}

		let client = try Client(
			address: accountAddress,
			v3Client: v3Client,
			dbPath: dbPath,
			installationID: v3Client.installationId().toHex,
			inboxID: v3Client.inboxId(),
			environment: options.api.env
		)

		// Register codecs
		for codec in options.codecs {
			client.register(codec: codec)
		}

		return client
	}

	public static func createV3(account: SigningKey, options: ClientOptions) async throws -> Client {
		let accountAddress = account.address.lowercased()
		let inboxId = try await getOrCreateInboxId(options: options, address: accountAddress)

		return try await initializeClient(
			accountAddress: accountAddress,
			options: options,
			signingKey: account,
			inboxId: inboxId
		)
	}
	
	public static func buildV3(address: String, options: ClientOptions) async throws -> Client {
		let accountAddress = address.lowercased()
		let inboxId = try await getOrCreateInboxId(options: options, address: accountAddress)

		return try await initializeClient(
			accountAddress: accountAddress,
			options: options,
			signingKey: nil,
			inboxId: inboxId
		)
	}

	static func initV3Client(
		accountAddress: String,
		options: ClientOptions?,
		privateKeyBundleV1: PrivateKeyBundleV1?,
		signingKey: SigningKey?,
		inboxId: String
	) async throws -> (FfiXmtpClient?, String) {
		if options?.enableV3 == true {
			let address = accountAddress.lowercased()
			
			let mlsDbDirectory = options?.dbDirectory
			var directoryURL: URL
			if let mlsDbDirectory = mlsDbDirectory {
				let fileManager = FileManager.default
				directoryURL = URL(fileURLWithPath: mlsDbDirectory, isDirectory: true)
				// Check if the directory exists, if not, create it
				if !fileManager.fileExists(atPath: directoryURL.path) {
					do {
						try fileManager.createDirectory(at: directoryURL, withIntermediateDirectories: true, attributes: nil)
					} catch {
						throw ClientError.creationError("Failed db directory \(mlsDbDirectory)")
					}
				}
			} else {
				directoryURL = URL.documentsDirectory
			}

			let alias = "xmtp-\(options?.api.env.rawValue ?? "")-\(inboxId).db3"
			let dbURL = directoryURL.appendingPathComponent(alias).path
			
			let encryptionKey = options?.dbEncryptionKey
			if (encryptionKey == nil) {
				throw ClientError.creationError("No encryption key passed for the database. Please store and provide a secure encryption key.")
			}

			let v3Client = try await LibXMTP.createClient(
				logger: XMTPLogger(),
				host: (options?.api.env ?? .local).url,
				isSecure: options?.api.env.isSecure == true,
				db: dbURL,
				encryptionKey: encryptionKey,
				inboxId: inboxId,
				accountAddress: address,
				nonce: 0,
				legacySignedPrivateKeyProto: try privateKeyBundleV1?.toV2().identityKey.serializedData(),
				historySyncUrl: options?.historySyncUrl
			)
			
            try await options?.preAuthenticateToInboxCallback?()
			if let signatureRequest = v3Client.signatureRequest() {
				if let signingKey = signingKey {
					do {
						if signingKey.type == WalletType.SCW {
							guard let chainId = signingKey.chainId else {
								throw ClientError.creationError("Chain id must be present to sign Smart Contract Wallet")
							}
							let signedData = try await signingKey.signSCW(message: signatureRequest.signatureText())
							try await signatureRequest.addScwSignature(signatureBytes: signedData,
																	   address: signingKey.address,
																	   chainId: UInt64(chainId),
																	   blockNumber: signingKey.blockNumber.flatMap { $0 >= 0 ? UInt64($0) : nil })

						} else {
							let signedData = try await signingKey.sign(message: signatureRequest.signatureText())
							try await signatureRequest.addEcdsaSignature(signatureBytes: signedData.rawData)
						}
						try await v3Client.registerIdentity(signatureRequest: signatureRequest)
					} catch {
						throw ClientError.creationError("Failed to sign the message: \(error.localizedDescription)")
					}
				} else {
					throw ClientError.creationError("No v3 keys found, you must pass a SigningKey in order to enable alpha MLS features")
				}
			}

			print("LibXMTP \(getVersionInfo())")

			return (v3Client, dbURL)
		} else {
			return (nil, "")
		}
	}

	static func create(account: SigningKey, apiClient: ApiClient, options: ClientOptions? = nil) async throws -> Client {
		let privateKeyBundleV1 = try await loadOrCreateKeys(for: account, apiClient: apiClient, options: options)
		let inboxId = try await getOrCreateInboxId(options: options ?? ClientOptions(), address: account.address)

		let (v3Client, dbPath) = try await initV3Client(
			accountAddress: account.address,
			options: options,
			privateKeyBundleV1: privateKeyBundleV1,
			signingKey: account,
			inboxId: inboxId
		)

		let client = try Client(address: account.address, privateKeyBundleV1: privateKeyBundleV1, apiClient: apiClient, v3Client: v3Client, dbPath: dbPath, installationID: v3Client?.installationId().toHex ?? "", inboxID: v3Client?.inboxId() ?? inboxId)
		let conversations = client.conversations
		let contacts = client.contacts
		try await client.ensureUserContactPublished()

		for codec in (options?.codecs ?? []) {
			client.register(codec: codec)
		}

		return client
	}

	static func loadOrCreateKeys(for account: SigningKey, apiClient: ApiClient, options: ClientOptions? = nil) async throws -> PrivateKeyBundleV1 {
		if let keys = try await loadPrivateKeys(for: account, apiClient: apiClient, options: options) {
			print("loading existing private keys.")
			#if DEBUG
				print("Loaded existing private keys.")
			#endif
			return keys
		} else {
			#if DEBUG
				print("No existing keys found, creating new bundle.")
			#endif
			let keys = try await PrivateKeyBundleV1.generate(wallet: account, options: options)
			let keyBundle = PrivateKeyBundle(v1: keys)
			let encryptedKeys = try await keyBundle.encrypted(with: account, preEnableIdentityCallback: options?.preEnableIdentityCallback)
			var authorizedIdentity = AuthorizedIdentity(privateKeyBundleV1: keys)
			authorizedIdentity.address = account.address
			let authToken = try await authorizedIdentity.createAuthToken()
			let apiClient = apiClient
			apiClient.setAuthToken(authToken)
			_ = try await apiClient.publish(envelopes: [
				Envelope(topic: .userPrivateStoreKeyBundle(account.address), timestamp: Date(), message: encryptedKeys.serializedData()),
			])

			return keys
		}
	}

	static func loadPrivateKeys(for account: SigningKey, apiClient: ApiClient, options: ClientOptions? = nil) async throws -> PrivateKeyBundleV1? {
		let res = try await apiClient.query(
			topic: .userPrivateStoreKeyBundle(account.address),
			pagination: nil
		)

		for envelope in res.envelopes {
			let encryptedBundle = try EncryptedPrivateKeyBundle(serializedData: envelope.message)
			let bundle = try await encryptedBundle.decrypted(with: account, preEnableIdentityCallback: options?.preEnableIdentityCallback)
			if case .v1 = bundle.version {
				return bundle.v1
			}
			print("discarding unsupported stored key bundle")
		}

		return nil
	}
	
	public static func getOrCreateInboxId(options: ClientOptions, address: String) async throws -> String {
		var inboxId: String
		do {
			inboxId = try await getInboxIdForAddress(
				logger: XMTPLogger(),
				host: options.api.env.url,
				isSecure: options.api.env.isSecure == true,
				accountAddress: address
			) ?? generateInboxId(accountAddress: address, nonce: 0)
		} catch {
			inboxId = generateInboxId(accountAddress: address, nonce: 0)
		}
		return inboxId
	}

	public func canMessageV3(address: String) async throws -> Bool {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}
		let canMessage = try await client.canMessage(accountAddresses: [address])
		return canMessage[address.lowercased()] ?? false
	}

	public func canMessageV3(addresses: [String]) async throws -> [String: Bool]  {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}

		return try await client.canMessage(accountAddresses: addresses)
	}

	public static func from(bundle: PrivateKeyBundle, options: ClientOptions? = nil) async throws -> Client {
		return try await from(v1Bundle: bundle.v1, options: options)
	}

	/// Create a Client from saved v1 key bundle.
	public static func from(
		v1Bundle: PrivateKeyBundleV1,
		options: ClientOptions? = nil,
		signingKey: SigningKey? = nil
	) async throws -> Client {
		let address = try v1Bundle.identityKey.publicKey.recoverWalletSignerPublicKey().walletAddress
		let options = options ?? ClientOptions()
		
		let inboxId = try await getOrCreateInboxId(options: options, address: address)

		let (v3Client, dbPath) = try await initV3Client(
			accountAddress: address,
			options: options,
			privateKeyBundleV1: v1Bundle,
			signingKey: nil,
			inboxId: inboxId
		)

		let client = try await LibXMTP.createV2Client(host: options.api.env.url, isSecure: options.api.env.isSecure)
		let apiClient = try GRPCApiClient(
			environment: options.api.env,
			secure: options.api.isSecure,
			rustClient: client
		)

		let result = try Client(address: address, privateKeyBundleV1: v1Bundle, apiClient: apiClient, v3Client: v3Client, dbPath: dbPath, installationID: v3Client?.installationId().toHex ?? "", inboxID: v3Client?.inboxId() ?? inboxId)
		let conversations = result.conversations
		let contacts = result.contacts
		for codec in options.codecs {
			result.register(codec: codec)
		}

		return result
	}

	init(address: String, privateKeyBundleV1: PrivateKeyBundleV1, apiClient: ApiClient, v3Client: LibXMTP.FfiXmtpClient?, dbPath: String = "", installationID: String, inboxID: String) throws {
		self.address = address
		self.privateKeyBundleV1 = privateKeyBundleV1
		self.apiClient = apiClient
		self.v3Client = v3Client
		self.dbPath = dbPath
		self.installationID = installationID
		self.inboxID = inboxID
		self.hasV2Client = true
		self.environment = apiClient.environment
	}
	
	init(address: String, v3Client: LibXMTP.FfiXmtpClient, dbPath: String, installationID: String, inboxID: String, environment: XMTPEnvironment) throws {
		self.address = address
		self.v3Client = v3Client
		self.dbPath = dbPath
		self.installationID = installationID
		self.inboxID = inboxID
		self.hasV2Client = false
		self.environment = environment
	}

	public var privateKeyBundle: PrivateKeyBundle {
		get throws {
			try PrivateKeyBundle(v1: v1keys)
		}
	}

	public var publicKeyBundle: SignedPublicKeyBundle {
		get throws {
			try v1keys.toV2().getPublicKeyBundle()
		}
	}

	public var v1keys: PrivateKeyBundleV1 {
		get throws {
			guard let keys = privateKeyBundleV1 else {
				throw ClientError.noV2Client("Error no V2 client initialized")
			}
			return keys
		}
	}

	public var keys: PrivateKeyBundleV2 {
		get throws {
			try v1keys.toV2()
		}
	}

	public func canMessage(_ peerAddress: String) async throws -> Bool {
		return try await query(topic: .contact(peerAddress)).envelopes.count > 0
	}

	public static func canMessage(_ peerAddress: String, options: ClientOptions? = nil) async throws -> Bool {
		let options = options ?? ClientOptions()
		let client = try await LibXMTP.createV2Client(host: options.api.env.url, isSecure: options.api.env.isSecure)
		let apiClient = try GRPCApiClient(
			environment: options.api.env,
			secure: options.api.isSecure,
			rustClient: client
		)
		return try await apiClient.query(topic: Topic.contact(peerAddress)).envelopes.count > 0
	}

	public func importConversation(from conversationData: Data) throws -> Conversation? {
		let jsonDecoder = JSONDecoder()

		do {
			let v2Export = try jsonDecoder.decode(ConversationV2Export.self, from: conversationData)
			return try importV2Conversation(export: v2Export)
		} catch {
			do {
				let v1Export = try jsonDecoder.decode(ConversationV1Export.self, from: conversationData)
				return try importV1Conversation(export: v1Export)
			} catch {
				throw ConversationImportError.invalidData
			}
		}
	}

	func importV2Conversation(export: ConversationV2Export) throws -> Conversation {
		guard let keyMaterial = Data(base64Encoded: Data(export.keyMaterial.utf8)) else {
			throw ConversationImportError.invalidData
		}

        var consentProof: ConsentProofPayload? = nil
        if let exportConsentProof = export.consentProof {
            var proof = ConsentProofPayload()
            proof.signature = exportConsentProof.signature
            proof.timestamp = exportConsentProof.timestamp
            proof.payloadVersion = ConsentProofPayloadVersion.consentProofPayloadVersion1
            consentProof = proof
        }

		return .v2(ConversationV2(
			topic: export.topic,
			keyMaterial: keyMaterial,
			context: InvitationV1.Context(
				conversationID: export.context?.conversationId ?? "",
				metadata: export.context?.metadata ?? [:]
			),
			peerAddress: export.peerAddress,
			client: self,
			header: SealedInvitationHeaderV1(),
            consentProof: consentProof
		))
	}

	func importV1Conversation(export: ConversationV1Export) throws -> Conversation {
		let formatter = ISO8601DateFormatter()
		formatter.formatOptions.insert(.withFractionalSeconds)

		guard let sentAt = formatter.date(from: export.createdAt) else {
			throw ConversationImportError.invalidData
		}

		return .v1(ConversationV1(
			client: self,
			peerAddress: export.peerAddress,
			sentAt: sentAt
		))
	}

	func ensureUserContactPublished() async throws {
		if let contact = try await getUserContact(peerAddress: address),
		   case .v2 = contact.version,
		   try keys.getPublicKeyBundle().equals(contact.v2.keyBundle)
		{
			return
		}

		try await publishUserContact(legacy: true)
	}

	func publishUserContact(legacy: Bool = false) async throws {
		var envelopes: [Envelope] = []

		if legacy {
			var contactBundle = ContactBundle()
			contactBundle.v1.keyBundle = try v1keys.toPublicKeyBundle()

			var envelope = Envelope()
			envelope.contentTopic = Topic.contact(address).description
			envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch * 1_000_000)
			envelope.message = try contactBundle.serializedData()

			envelopes.append(envelope)
		}

		var contactBundle = ContactBundle()
		contactBundle.v2.keyBundle = try keys.getPublicKeyBundle()
		contactBundle.v2.keyBundle.identityKey.signature.ensureWalletSignature()

		var envelope = Envelope()
		envelope.contentTopic = Topic.contact(address).description
		envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch * 1_000_000)
		envelope.message = try contactBundle.serializedData()
		envelopes.append(envelope)

		_ = try await publish(envelopes: envelopes)
	}

	public func query(topic: Topic, pagination: Pagination? = nil) async throws -> QueryResponse {
		guard let client = apiClient else {
			throw ClientError.noV2Client("Error no V2 client initialized")
		}
		return try await client.query(
			topic: topic,
			pagination: pagination
		)
	}

	public func batchQuery(request: BatchQueryRequest) async throws -> BatchQueryResponse {
		guard let client = apiClient else {
			throw ClientError.noV2Client("Error no V2 client initialized")
		}
		return try await client.batchQuery(request: request)
	}

	public func publish(envelopes: [Envelope]) async throws {
		guard let client = apiClient else {
			throw ClientError.noV2Client("Error no V2 client initialized")
		}
		let authorized = try AuthorizedIdentity(address: address, authorized: v1keys.identityKey.publicKey, identity: v1keys.identityKey)
		let authToken = try await authorized.createAuthToken()

		client.setAuthToken(authToken)

		try await client.publish(envelopes: envelopes)
	}

	public func subscribe(
		topics: [String],
		callback: FfiV2SubscriptionCallback
	) async throws -> FfiV2Subscription {
		return try await subscribe2(request: FfiV2SubscribeRequest(contentTopics: topics), callback: callback)
	}

	public func subscribe2(
		request: FfiV2SubscribeRequest,
		callback: FfiV2SubscriptionCallback
	) async throws -> FfiV2Subscription {
		guard let client = apiClient else {
			throw ClientError.noV2Client("Error no V2 client initialized")
		}
		return try await client.subscribe(request: request, callback: callback)
	}

	public func deleteLocalDatabase() throws {
		try dropLocalDatabaseConnection()
		let fm = FileManager.default
		try fm.removeItem(atPath: dbPath)
	}
	
	@available(*, deprecated, message: "This function is delicate and should be used with caution. App will error if database not properly reconnected. See: reconnectLocalDatabase()")
	public func dropLocalDatabaseConnection() throws {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}
		try client.releaseDbConnection()
	}
	
	public func reconnectLocalDatabase() async throws {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}
		try await client.dbReconnect()
	}

	func getUserContact(peerAddress: String) async throws -> ContactBundle? {
		let peerAddress = EthereumAddress(peerAddress).toChecksumAddress()
		return try await contacts.find(peerAddress)
	}
	
	public func inboxIdFromAddress(address: String) async throws -> String? {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}
		return try await client.findInboxId(address: address.lowercased())
	}
	
	public func findGroup(groupId: String) throws -> Group? {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}
		do {
			return Group(ffiGroup: try client.conversation(conversationId: groupId.hexToData), client: self)
		} catch {
			return nil
		}
	}
	
	public func findConversation(conversationId: String) throws -> Conversation? {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}
		do {
			let conversation = try client.conversation(conversationId: conversationId.hexToData)
			return try conversation.toConversation(client: self)
		} catch {
			return nil
		}
	}
	
	public func findConversationByTopic(topic: String) throws -> Conversation? {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}
		do {
			let regexPattern = #"/xmtp/mls/1/g-(.*?)/proto"#
			if let regex = try? NSRegularExpression(pattern: regexPattern) {
				let range = NSRange(location: 0, length: topic.utf16.count)
				if let match = regex.firstMatch(in: topic, options: [], range: range) {
					let conversationId = (topic as NSString).substring(with: match.range(at: 1))
					let conversation = try client.conversation(conversationId: conversationId.hexToData)
					return try conversation.toConversation(client: self)
				}
			}
		} catch {
			return nil
		}
		return nil
	}
	
	public func findDm(address: String) async throws -> Dm? {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}
		guard let inboxId = try await inboxIdFromAddress(address: address) else {
			throw ClientError.creationError("No inboxId present")
		}
		do {
			let conversation = try client.dmConversation(targetInboxId: inboxId)
			return Dm(ffiConversation: conversation, client: self)
		} catch {
			return nil
		}
	}

	public func findMessage(messageId: String) throws -> MessageV3? {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}
		do {
			return MessageV3(client: self, ffiMessage: try client.message(messageId: messageId.hexToData))
		} catch {
			return nil
		}
	}
	
	public func requestMessageHistorySync() async throws {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error no V3 client initialized")
		}
		try await client.requestHistorySync()
	}
	
	public func revokeAllOtherInstallations(signingKey: SigningKey) async throws {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error: No V3 client initialized")
		}
		
		let signatureRequest = try await client.revokeAllOtherInstallations()
		do {
			let signedData = try await signingKey.sign(message: signatureRequest.signatureText())
			try await signatureRequest.addEcdsaSignature(signatureBytes: signedData.rawData)
			try await client.applySignatureRequest(signatureRequest: signatureRequest)
		} catch {
			throw ClientError.creationError("Failed to sign the message: \(error.localizedDescription)")
		}
	}
	
	public func inboxState(refreshFromNetwork: Bool) async throws -> InboxState {
		guard let client = v3Client else {
			throw ClientError.noV3Client("Error: No V3 client initialized")
		}
		return InboxState(ffiInboxState: try await client.inboxState(refreshFromNetwork: refreshFromNetwork))
	}
}
