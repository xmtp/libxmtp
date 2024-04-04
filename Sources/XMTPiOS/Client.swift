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

public enum ClientError: Error, CustomStringConvertible {
	case creationError(String)

	public var description: String {
		switch self {
		case .creationError(let err):
			return "ClientError.creationError: \(err)"
		}
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

	public var mlsAlpha = false
	public var mlsEncryptionKey: Data?
	public var mlsDbPath: String?

	public init(
		api: Api = Api(),
		codecs: [any ContentCodec] = [],
		preEnableIdentityCallback: PreEventCallback? = nil,
		preCreateIdentityCallback: PreEventCallback? = nil,
		mlsAlpha: Bool = false,
		mlsEncryptionKey: Data? = nil,
		mlsDbPath: String? = nil
	) {
		self.api = api
		self.codecs = codecs
		self.preEnableIdentityCallback = preEnableIdentityCallback
		self.preCreateIdentityCallback = preCreateIdentityCallback
		self.mlsAlpha = mlsAlpha
		self.mlsEncryptionKey = mlsEncryptionKey
		self.mlsDbPath = mlsDbPath
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
	let privateKeyBundleV1: PrivateKeyBundleV1
	let apiClient: ApiClient
	let v3Client: LibXMTP.FfiXmtpClient?
	public let libXMTPVersion: String = getVersionInfo()
	let dbPath: String
	public let installationID: String

	/// Access ``Conversations`` for this Client.
	public lazy var conversations: Conversations = .init(client: self)

	/// Access ``Contacts`` for this Client.
	public lazy var contacts: Contacts = .init(client: self)

	/// The XMTP environment which specifies which network this Client is connected to.
	public var environment: XMTPEnvironment {
		apiClient.environment
	}

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
			throw ClientError.creationError("\(error)")
		}
	}

	static func initV3Client(
		address: String,
		options: ClientOptions?,
		source: LegacyIdentitySource,
		privateKeyBundleV1: PrivateKeyBundleV1,
		signingKey: SigningKey?
	) async throws -> (FfiXmtpClient?, String) {
		if options?.mlsAlpha == true, options?.api.env.supportsMLS == true {
			let dbURL = options?.mlsDbPath ?? URL.documentsDirectory.appendingPathComponent("xmtp-\(options?.api.env.rawValue ?? "")-\(address).db3").path

			var encryptionKey = options?.mlsEncryptionKey
			if (encryptionKey == nil) {
				throw ClientError.creationError("No encryption key passed for the database. Please store and provide a secure encryption key.")
			}

			let v3Client = try await LibXMTP.createClient(
				logger: XMTPLogger(),
				host: (options?.api.env ?? .local).url,
				isSecure: options?.api.env.isSecure == true,
				db: dbURL,
				encryptionKey: encryptionKey,
				accountAddress: address,
				legacyIdentitySource: source,
				legacySignedPrivateKeyProto: try privateKeyBundleV1.toV2().identityKey.serializedData()
			)

			if let textToSign = v3Client.textToSign() {
				guard let signingKey else {
					throw ClientError.creationError("No v3 keys found, you must pass a SigningKey in order to enable alpha MLS features")
				}

				let signature = try await signingKey.sign(message: textToSign)
				try await v3Client.registerIdentity(recoverableWalletSignature: signature.rawData)
			} else {
				try await v3Client.registerIdentity(recoverableWalletSignature: nil)
			}

			print("LibXMTP \(getVersionInfo())")

			return (v3Client, dbURL)
		} else {
			return (nil, "")
		}
	}

	static func create(account: SigningKey, apiClient: ApiClient, options: ClientOptions? = nil) async throws -> Client {
		let (privateKeyBundleV1, source) = try await loadOrCreateKeys(for: account, apiClient: apiClient, options: options)

		let (v3Client, dbPath) = try await initV3Client(
			address: account.address,
			options: options,
			source: source,
			privateKeyBundleV1: privateKeyBundleV1,
			signingKey: account
		)

		let client = try Client(address: account.address, privateKeyBundleV1: privateKeyBundleV1, apiClient: apiClient, v3Client: v3Client, dbPath: dbPath, installationID: v3Client?.installationId().toHex ?? "")
		let conversations = client.conversations
		let contacts = client.contacts
		try await client.ensureUserContactPublished()

		for codec in (options?.codecs ?? []) {
			client.register(codec: codec)
		}

		return client
	}

	static func loadOrCreateKeys(for account: SigningKey, apiClient: ApiClient, options: ClientOptions? = nil) async throws -> (PrivateKeyBundleV1, LegacyIdentitySource) {
		if let keys = try await loadPrivateKeys(for: account, apiClient: apiClient, options: options) {
			print("loading existing private keys.")
			#if DEBUG
				print("Loaded existing private keys.")
			#endif
			return (keys, .network)
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

			return (keys, .keyGenerator)
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

	public func canMessageV3(address: String) async throws -> Bool {
		guard let v3Client else {
			return false
		}

		return try await v3Client.canMessage(accountAddresses: [address]) == [true]
	}

	public func canMessageV3(addresses: [String]) async throws -> Bool {
		guard let v3Client else {
			return false
		}

		return try await !v3Client.canMessage(accountAddresses: addresses).contains(false)
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

		let (v3Client, dbPath) = try await initV3Client(
			address: address,
			options: options,
			source: .static,
			privateKeyBundleV1: v1Bundle,
			signingKey: nil
		)

		let client = try await LibXMTP.createV2Client(host: options.api.env.url, isSecure: options.api.env.isSecure)
		let apiClient = try GRPCApiClient(
			environment: options.api.env,
			secure: options.api.isSecure,
			rustClient: client
		)

		let result = try Client(address: address, privateKeyBundleV1: v1Bundle, apiClient: apiClient, v3Client: v3Client, dbPath: dbPath, installationID: v3Client?.installationId().toHex ?? "")
		let conversations = result.conversations
		let contacts = result.contacts
		for codec in options.codecs {
			result.register(codec: codec)
		}

		return result
	}

	init(address: String, privateKeyBundleV1: PrivateKeyBundleV1, apiClient: ApiClient, v3Client: LibXMTP.FfiXmtpClient?, dbPath: String = "", installationID: String) throws {
		self.address = address
		self.privateKeyBundleV1 = privateKeyBundleV1
		self.apiClient = apiClient
		self.v3Client = v3Client
		self.dbPath = dbPath
		self.installationID = installationID
	}

	public var privateKeyBundle: PrivateKeyBundle {
		PrivateKeyBundle(v1: privateKeyBundleV1)
	}

	public var publicKeyBundle: SignedPublicKeyBundle {
		privateKeyBundleV1.toV2().getPublicKeyBundle()
	}

	public var v1keys: PrivateKeyBundleV1 {
		privateKeyBundleV1
	}

	public var keys: PrivateKeyBundleV2 {
		privateKeyBundleV1.toV2()
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

		return .v2(ConversationV2(
			topic: export.topic,
			keyMaterial: keyMaterial,
			context: InvitationV1.Context(
				conversationID: export.context?.conversationId ?? "",
				metadata: export.context?.metadata ?? [:]
			),
			peerAddress: export.peerAddress,
			client: self,
			header: SealedInvitationHeaderV1()
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
		   keys.getPublicKeyBundle().equals(contact.v2.keyBundle)
		{
			return
		}

		try await publishUserContact(legacy: true)
	}

	func publishUserContact(legacy: Bool = false) async throws {
		var envelopes: [Envelope] = []

		if legacy {
			var contactBundle = ContactBundle()
			contactBundle.v1.keyBundle = privateKeyBundleV1.toPublicKeyBundle()

			var envelope = Envelope()
			envelope.contentTopic = Topic.contact(address).description
			envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch * 1_000_000)
			envelope.message = try contactBundle.serializedData()

			envelopes.append(envelope)
		}

		var contactBundle = ContactBundle()
		contactBundle.v2.keyBundle = keys.getPublicKeyBundle()
		contactBundle.v2.keyBundle.identityKey.signature.ensureWalletSignature()

		var envelope = Envelope()
		envelope.contentTopic = Topic.contact(address).description
		envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch * 1_000_000)
		envelope.message = try contactBundle.serializedData()
		envelopes.append(envelope)

		_ = try await publish(envelopes: envelopes)
	}

	public func query(topic: Topic, pagination: Pagination? = nil) async throws -> QueryResponse {
		return try await apiClient.query(
			topic: topic,
			pagination: pagination
		)
	}

	public func batchQuery(request: BatchQueryRequest) async throws -> BatchQueryResponse {
		return try await apiClient.batchQuery(request: request)
	}

	public func publish(envelopes: [Envelope]) async throws {
		let authorized = AuthorizedIdentity(address: address, authorized: privateKeyBundleV1.identityKey.publicKey, identity: privateKeyBundleV1.identityKey)
		let authToken = try await authorized.createAuthToken()

		apiClient.setAuthToken(authToken)

		try await apiClient.publish(envelopes: envelopes)
	}

	public func subscribe(topics: [String]) -> AsyncThrowingStream<Envelope, Error> {
		return apiClient.subscribe(topics: topics)
	}

	public func subscribe(topics: [Topic]) -> AsyncThrowingStream<Envelope, Error> {
		return subscribe(topics: topics.map(\.description))
	}

	public func deleteLocalDatabase() throws {
		let fm = FileManager.default
		try fm.removeItem(atPath: dbPath)
	}

	func getUserContact(peerAddress: String) async throws -> ContactBundle? {
		let peerAddress = EthereumAddress(peerAddress).toChecksumAddress()
		return try await contacts.find(peerAddress)
	}
}
