import Foundation

/// A chat route instance ends when a sheet or another destination becomes current.
public struct ChatDestination: Hashable, Sendable {
	public let conversationID: String
	public let session: SessionKey
	public let token: UInt64
	public var origin: DestinationOrigin {
		DestinationOrigin(session: session, token: token)
	}

	@MainActor public init?(conversationID: String, session: SessionKey?, routes: RouteOwner) {
		guard let session, routes.current == .chat(conversationID) else { return nil }
		self.conversationID = conversationID
		self.session = session
		token = routes.token
	}
}
