import SwiftUI
import XMTPiOS
import OSLog
import web3swift

// The user's authenticated session with XMTP.
//
// This is how the Views can observe messaging data
// and interact with the XmtpClient.
@Observable
class XmtpSession {
    private static let logger = Logger.forClass(XmtpSession.self)
    enum State {
        case loading
        case loggedOut
        case loggedIn
    }
    
    private(set) var state: State = .loading
    var inboxId: String? {
        client?.inboxID
    }
    private(set) var conversationIds: [String] = []
    let conversations = ObservableCache<Conversation>(defaultValue: nil)
    let conversationMembers = ObservableCache<[Member]>(defaultValue: [])
    let conversationMessages = ObservableCache<[DecodedMessage]>(defaultValue: [])
    let inboxes = ObservableCache<InboxState>(defaultValue: nil)
    
    private var client: Client?
    
    init() {
        // TODO: check for saved credentials from the keychain
        state = .loggedOut
        conversations.loader = { conversationId in
            try await self.client!.conversations.findConversation(conversationId: conversationId)!
        }
        conversationMembers.loader = { conversationId in
            if self.client == nil {
                return []
            }
            let c = try await self.client!.conversations.findConversation(conversationId: conversationId)!
            return try await c.members()
        }
        conversationMessages.loader = { conversationId in
            if self.client == nil {
                return []
            }
            let c = try await self.client!.conversations.findConversation(conversationId: conversationId)!
            return try await c.messages(limit: 10) // TODO paging etc.
        }
        inboxes.loader = { inboxId in
            try await self.client!.inboxStatesForInboxIds(
                refreshFromNetwork: true, // TODO: consider false sometimes?
                inboxIds: [inboxId]).first! // there's only one.
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
        
        // TODO: accept as params
        // TODO: use real account
        let account = try PrivateKey.generate()
        let dbKey = Data((0 ..< 32)
            .map { _ in UInt8.random(in: UInt8.min ... UInt8.max) })
        
        // To re-use a randomly generated account during dev,
        // copy these from the logs of the first run:
        //        let account = PrivateKey(jsonString: "...")
        //        let dbKey = Data(base64Encoded: "...")
        Self.logger.trace("dbKey: \(dbKey.base64EncodedString())")
        Self.logger.trace("account: \(try! account.jsonString())")
        
        client = try await Client.create(account: account, options: ClientOptions(dbEncryptionKey: dbKey))
        Self.logger.trace("inboxID: \((self.client?.inboxID) ?? "?")")
        
        // TODO: save credentials in the keychain
    }
    
    func refreshConversations() async throws {
        Self.logger.debug("refreshConversations")
        _ = try await client?.conversations.syncAllConversations()
        let conversations = (try? await client?.conversations.list()) ?? []  // TODO: paging etc.
        self.conversationIds = conversations.map { $0.id }
    }
    
    func refreshConversation(conversationId: String) async throws {
        Self.logger.debug("refreshConversation \(conversationId)")
        guard let c = try await client?.conversations.findConversation(conversationId: conversationId) else {
            return // TODO: consider logging failure instead
        }
        try await c.sync()
        _ = await [
            try conversations.reload(conversationId).result.get(),
            try conversationMessages.reload(conversationId).result.get(),
            try conversationMembers.reload(conversationId).result.get()
        ] as [Any?]
    }

    func sendMessage(_ message: String, to conversationId: String) async throws -> Bool {
        Self.logger.debug("sendMessage \(message) to \(conversationId)")
        guard let c = try await client?.conversations.findConversation(conversationId: conversationId) else {
            return false // TODO: consider logging failure instead
        }
        guard let _ = try? await c.send(text: message) else {
            return false
        }
        _ = conversationMessages.reload(conversationId) // TODO: consider try/awaiting the roundtrip here
        return true
    }

    func clear() async throws {
        Self.logger.debug("clear")
        conversationIds = []
        conversations.clear()
        conversationMembers.clear()
        conversationMessages.clear()
        inboxes.clear()
        // TODO: clear saved credentials etc
        client = nil
        state = .loggedOut
    }
}

extension Conversation {
    var name: String? {
        if case .group(let g) = self {
            return try? g.name()
        }
        return nil
    }
}
