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
				return try await c.messages(options: nil) // TODO: paging etc.
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

		let signer = await generateLocalSigner()
		let backendUrl = ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://localhost:5050"
		client = try await SDKClient.create(
			signer: signer,
			options: ClientOptions(
				backend: .options(options: BackendOptions(url: backendUrl)),
				storage: StorageOptions(location: .default),
			),
		)
	}

	func refreshConversations() async throws {
		Self.logger.debug("refreshConversations")
		_ = try await client?.conversations().syncAll(consentStates: nil)
		let conversations = try await client?.conversations().list() ?? [] // TODO: Add pagination.
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
		conversationIds = []
		conversations.clear()
		conversationMembers.clear()
		conversationMessages.clear()
		inboxes.clear()
		// TODO: clear saved credentials etc
		try await client?.end()
		client = nil
		state = .loggedOut
	}
}

extension Conversation {
	var name: String {
		id()
	}
}
