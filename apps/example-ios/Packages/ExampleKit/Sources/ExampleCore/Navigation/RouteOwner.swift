import Foundation
import Observation

public enum Route: Hashable, Codable, Sendable {
	case connect, chooseHandle, chats, requests, newConversation
	case chat(String), details(String), member(conversationID: String, inboxID: String)
	case permissions(String), disappearing(String), sharedMedia(String), groupFields(String), myFields(String)
	case reactions(conversationID: String, messageID: String, emoji: String)
	case emojiPicker(conversationID: String, messageID: String)
	case settings, changeHandle, attachmentRecovery
	public var isSheet: Bool {
		switch self {
		case .newConversation, .member, .reactions, .emojiPicker: true
		default: false
		}
	}
}

@MainActor @Observable public final class RouteOwner {
	public private(set) var path: [Route] = []
	public private(set) var root: Route = .connect
	public private(set) var sheet: Route?
	public private(set) var token: UInt64 = 0
	public init() {}
	public var current: Route {
		sheet ?? path.last ?? root
	}

	public func show(_ route: Route) {
		token += 1; if route.isSheet {
			sheet = route
		} else {
			sheet = nil; path.append(route)
		}
	}

	public func replace(_ route: Route) {
		token += 1; sheet = nil; path = []; root = route
	}

	public func back() {
		token += 1; if sheet != nil {
			sheet = nil
		} else if !path.isEmpty {
			path.removeLast()
		}
	}

	public func dismissSheet() {
		guard sheet != nil else { return }
		token += 1
		sheet = nil
	}

	public func setPath(_ path: [Route]) {
		guard self.path != path else { return }
		token += 1; sheet = nil; self.path = path
	}

	public func admit(_ origin: DestinationOrigin, fence: SessionFence) -> Bool {
		token == origin.token && fence.accepts(origin.session)
	}
}
