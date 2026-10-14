import Foundation

/// A conversation write pauses only its own alert admission and late completions.
@MainActor final class PushConversationPrivacy {
	private var writes: [String: Int] = [:]
	private var revisions: [String: UInt64] = [:]
	func begin(_ id: String) {
		writes[id, default: 0] += 1; revisions[id, default: 0] &+= 1
	}

	func end(_ id: String) {
		writes[id, default: 0] -= 1
		if writes[id] == 0 {
			writes.removeValue(forKey: id)
		}
		revisions[id, default: 0] &+= 1
	}

	func revision(for id: String?) -> UInt64 {
		id.map { revisions[$0, default: 0] } ?? 0
	}

	func admits(_ id: String?, revision: UInt64) -> Bool {
		guard let id else { return true }
		return writes[id, default: 0] == 0 && self.revision(for: id) == revision
	}
}
