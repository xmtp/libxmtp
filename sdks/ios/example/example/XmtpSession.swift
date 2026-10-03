import OSLog
import SwiftUI
import XmtpSdk

/// The user's authenticated session with XMTP.
///
/// Views read messaging data and call the SDK through this session.
@Observable
class XmtpSession {
	private static let logger = Logger.forClass(XmtpSession.self)
	enum State {
		case loading
		case loggedOut
		case loggedIn
	}

	enum XmtpSessionError: Error {
		case notInitialized
		case unableToLoadData
	}

	private(set) var state: State = .loading
	var inboxId: String? {
		client?.inboxId()
	}

	private(set) var conversationIds: [String] = []
	let conversations = ObservableCache<Conversation>(defaultValue: nil)
	let conversationNames = ObservableCache<String>(defaultValue: "Conversation")
	let conversationMembers = ObservableCache<[Member]>(defaultValue: [])
	let conversationMessages = ObservableCache<[Message]>(defaultValue: [])
	let inboxes = ObservableCache<InboxState>(defaultValue: nil)

	private var client: SDKClient?

	init() {
		// TODO: check for saved credentials from the keychain
		state = .loggedOut
		conversations.loader = { conversationId in
			guard let client = self.client else {
				throw XmtpSessionError.notInitialized
			}
			if let c = try await client.conversations().getById(id: conversationId) {
				return c
			}
			throw XmtpSessionError.unableToLoadData
		}
		conversationNames.loader = { conversationId in
			guard let conversation = try await self.conversations.reload(conversationId).value else {
				throw XmtpSessionError.unableToLoadData
			}
			return try await conversation.displayName()
		}
		conversationMembers.loader = { conversationId in
			guard let client = self.client else {
				return []
			}
			if let c = try await client.conversations().getById(id: conversationId) {
				return try await c.members()
			}
			return []
		}
		conversationMessages.loader = { conversationId in
			guard let client = self.client else {
				return []
			}
			if let c = try await client.conversations().getById(id: conversationId) {
				return try await c.messages(options: ListMessagesOptions(limit: 10)) // TODO: paging etc.
			}
			return []
		}
		inboxes.loader = { inboxId in
			guard let client = self.client else {
				throw XmtpSessionError.notInitialized
			}
			if let inbox = try await client.inboxStates(ids: [inboxId], refreshFromNetwork: true).first // there's only one.
			{
				return inbox
			}
			throw XmtpSessionError.unableToLoadData
		}
	}

	func login() async throws {
		Self.logger.debug("login")
		guard state == .loggedOut else { return }
		state = .loading
		defer {
			Self.logger.info("login \(self.client == nil ? "failed" : "succeeded")")
			state = client == nil ? .loggedOut : .loggedIn
		}

		guard let bundleId = Bundle.main.bundleIdentifier else {
			throw XmtpSessionError.unableToLoadData
		}
		let credentials = try await ExampleCredentials.loadOrCreate(service: bundleId)
		let signer = try await localSignerFromPrivateKey(key: credentials.signerKey)
		let backendUrl = ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://localhost:5050"
		client = try await SDKClient.create(
			signer: signer,
			options: ClientOptions(
				backend: .options(options: BackendOptions(url: backendUrl)),
				storage: StorageOptions(location: .default, encryptionKey: credentials.databaseKey),
			),
		)
	}

	func refreshConversations() async throws {
		Self.logger.debug("refreshConversations")
		_ = try await client?.conversations().syncAll(consentStates: nil)
		let conversations = try await client?.conversations().list() ?? [] // TODO: Add pagination.
		for conversation in conversations {
			self.conversations.insert(identifier: conversation.id(), value: conversation)
			try await conversationNames.insert(identifier: conversation.id(), value: conversation.displayName())
		}
		conversationIds = conversations.map { $0.id() }
	}

	func refreshConversation(conversationId: String) async throws {
		Self.logger.debug("refreshConversation \(conversationId)")
		guard let c = try await client?.conversations().getById(id: conversationId) else {
			return // TODO: consider logging failure instead
		}
		try await c.sync()
		_ = try await [
			conversations.reload(conversationId).result.get(),
			conversationNames.reload(conversationId).result.get(),
			conversationMessages.reload(conversationId).result.get(),
			conversationMembers.reload(conversationId).result.get(),
		] as [Any?]
	}

	func sendMessage(_ message: String, to conversationId: String) async throws -> Bool {
		Self.logger.debug("Send a message to \(conversationId)")
		guard let c = try await client?.conversations().getById(id: conversationId) else {
			return false // TODO: consider logging failure instead
		}
		_ = try await c.sendText(text: message, options: nil)
		_ = conversationMessages.reload(conversationId) // TODO: consider try/awaiting the roundtrip here
		return true
	}

	func createConversation(peer: String, isGroup: Bool) async throws -> String {
		guard let client else { throw XmtpSessionError.notInitialized }
		let conversation: Conversation = if isGroup {
			try await .group(group: client.conversations().createGroup(members: [peer], options: nil))
		} else {
			try await .dm(dm: client.conversations().createDm(peer: peer, options: nil))
		}
		try await refreshConversations()
		return conversation.id()
	}

	func clear() async throws {
		Self.logger.debug("clear")
		try await client?.end()
		conversationIds = []
		conversations.clear()
		conversationNames.clear()
		conversationMembers.clear()
		conversationMessages.clear()
		inboxes.clear()
		// Keep the keys so the next login opens the same encrypted database.
		client = nil
		state = .loggedOut
	}
}

extension Conversation {
	func displayName() async throws -> String {
		switch self {
		case let .group(group):
			let name = try await group.state().name
			return name.isEmpty ? "Untitled group" : name
		case .dm:
			return "Direct message"
		}
	}
}
