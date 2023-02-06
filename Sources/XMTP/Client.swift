//
//  Client.swift
//
//
//  Created by Pat Nakajima on 11/22/22.
//

import Foundation
import GRPC
import XMTPProto

/// Specify configuration options for creating a ``Client``.
public struct ClientOptions {
	// Specify network options
	public struct Api {
		/// Specify which XMTP network to connect to. Defaults to ``.dev``
		public var env: XMTPEnvironment = .dev

		/// Specify whether the API client should use TLS security. In general this should only be false when using the `.local` environment.
		public var isSecure: Bool = true

		public init(env: XMTPEnvironment = .dev, isSecure: Bool = true) {
			self.env = env
			self.isSecure = isSecure
		}
	}

	public var api = Api()
	public var codecs: [any ContentCodec] = []

	public init(api: Api = Api(), codecs: [any ContentCodec] = []) {
		self.api = api
		self.codecs = codecs
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
public class Client {
	/// The wallet address of the ``SigningKey`` used to create this Client.
	public var address: String
	var privateKeyBundleV1: PrivateKeyBundleV1
	var apiClient: ApiClient

	/// Access ``Conversations`` for this Client.
	public lazy var conversations: Conversations = .init(client: self)

	/// Access ``Contacts`` for this Client.
	public lazy var contacts: Contacts = .init(client: self)

	/// The XMTP environment which specifies which network this Client is connected to.
	public var environment: XMTPEnvironment {
		apiClient.environment
	}

	static var codecRegistry = {
		var registry = CodecRegistry()
		registry.register(codec: TextCodec())
		return registry
	}()

	public static func register(codec: any ContentCodec) {
		codecRegistry.register(codec: codec)
	}

	/// Creates a client.
	public static func create(account: SigningKey, options: ClientOptions? = nil) async throws -> Client {
		let options = options ?? ClientOptions()

		let apiClient = try GRPCApiClient(
			environment: options.api.env,
			secure: options.api.isSecure
		)

		return try await create(account: account, apiClient: apiClient)
	}

	static func create(account: SigningKey, apiClient: ApiClient) async throws -> Client {
		let privateKeyBundleV1 = try await loadOrCreateKeys(for: account, apiClient: apiClient)

		let client = try Client(address: account.address, privateKeyBundleV1: privateKeyBundleV1, apiClient: apiClient)
		try await client.ensureUserContactPublished()

		return client
	}

	static func loadOrCreateKeys(for account: SigningKey, apiClient: ApiClient) async throws -> PrivateKeyBundleV1 {
		// swiftlint:disable no_optional_try
		if let keys = try await loadPrivateKeys(for: account, apiClient: apiClient) {
			// swiftlint:enable no_optional_try

			#if DEBUG
				print("Loaded existing private keys.")
			#endif
			return keys
		} else {
			#if DEBUG
				print("No existing keys found, creating new bundle.")
			#endif

			let keys = try await PrivateKeyBundleV1.generate(wallet: account)
			let keyBundle = PrivateKeyBundle(v1: keys)
			let encryptedKeys = try await keyBundle.encrypted(with: account)

			var authorizedIdentity = AuthorizedIdentity(privateKeyBundleV1: keys)
			authorizedIdentity.address = account.address
			let authToken = try await authorizedIdentity.createAuthToken()

			let apiClient = apiClient
			apiClient.setAuthToken(authToken)

			_ = try await apiClient.publish(envelopes: [
				Envelope(topic: .userPrivateStoreKeyBundle(account.address), timestamp: Date(), message: try encryptedKeys.serializedData()),
			])

			return keys
		}
	}

	static func loadPrivateKeys(for account: SigningKey, apiClient: ApiClient) async throws -> PrivateKeyBundleV1? {
		let topics: [Topic] = [.userPrivateStoreKeyBundle(account.address)]
		let res = try await apiClient.query(topics: topics, pagination: nil)

		for envelope in res.envelopes {
			let encryptedBundle = try EncryptedPrivateKeyBundle(serializedData: envelope.message)
			let bundle = try await encryptedBundle.decrypted(with: account)
			return bundle.v1
		}

		return nil
	}

	public static func from(bundle: PrivateKeyBundle, options: ClientOptions? = nil) throws -> Client {
		return try from(v1Bundle: bundle.v1, options: options)
	}

	/// Create a Client from saved v1 key bundle.
	public static func from(v1Bundle: PrivateKeyBundleV1, options: ClientOptions? = nil) throws -> Client {
		let address = try v1Bundle.identityKey.publicKey.recoverWalletSignerPublicKey().walletAddress

		let options = options ?? ClientOptions()

		let apiClient = try GRPCApiClient(
			environment: options.api.env,
			secure: options.api.isSecure
		)

		return try Client(address: address, privateKeyBundleV1: v1Bundle, apiClient: apiClient)
	}

	init(address: String, privateKeyBundleV1: PrivateKeyBundleV1, apiClient: ApiClient) throws {
		self.address = address
		self.privateKeyBundleV1 = privateKeyBundleV1
		self.apiClient = apiClient
	}

	public var privateKeyBundle: PrivateKeyBundle {
		PrivateKeyBundle(v1: privateKeyBundleV1)
	}

	public var v1keys: PrivateKeyBundleV1 {
		privateKeyBundleV1
	}

	public var keys: PrivateKeyBundleV2 {
		privateKeyBundleV1.toV2()
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

	func query(topics: [Topic], pagination: Pagination? = nil) async throws -> QueryResponse {
		return try await apiClient.query(topics: topics, pagination: pagination)
	}

	@discardableResult func publish(envelopes: [Envelope]) async throws -> PublishResponse {
		let authorized = AuthorizedIdentity(address: address, authorized: privateKeyBundleV1.identityKey.publicKey, identity: privateKeyBundleV1.identityKey)
		let authToken = try await authorized.createAuthToken()

		apiClient.setAuthToken(authToken)

		return try await apiClient.publish(envelopes: envelopes)
	}

	func subscribe(topics: [String]) -> AsyncThrowingStream<Envelope, Error> {
		return apiClient.subscribe(topics: topics)
	}

	func subscribe(topics: [Topic]) -> AsyncThrowingStream<Envelope, Error> {
		return subscribe(topics: topics.map(\.description))
	}

	func getUserContact(peerAddress: String) async throws -> ContactBundle? {
		return try await contacts.find(peerAddress)
	}
}
