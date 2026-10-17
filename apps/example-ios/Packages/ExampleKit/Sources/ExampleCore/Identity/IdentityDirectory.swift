import Foundation
import Observation

@MainActor @Observable public final class IdentityDirectory {
	public private(set) var identities: [String: InboxIdentity] = [:]
	private var revisions: [String: UInt64] = [:]
	private let queries: any IdentityQueries
	private let valid: @MainActor () -> Bool
	public init(queries: any IdentityQueries, valid: @escaping @MainActor () -> Bool) {
		self.queries = queries; self.valid = valid
	}

	public func merge(_ entries: [InboxIdentity]) {
		guard valid() else { return }; for entry in entries {
			identities[entry.inboxID] = entry; revisions[
				entry.inboxID,
				default: 0,
			] += 1
		}
	}

	public func handleChanged(inboxID: String, handle: String?, otherHandles: [String]) {
		let previous = identities[inboxID]
		let candidates = otherHandles.isEmpty ? [previous?.handle]
			.compactMap(\.self) + (previous?.otherHandles ?? []) : otherHandles
		var seen = Set<String>()
		let others = candidates.filter { $0 != handle && seen.insert($0).inserted }
		merge([InboxIdentity(inboxID: inboxID, handle: handle, otherHandles: others)])
	}

	public func load(_ inboxIDs: [String]) async throws {
		let captured = Dictionary(uniqueKeysWithValues: Set(inboxIDs).map { ($0, revisions[$0, default: 0]) })
		let entries = try await queries.handles(inboxIDs: inboxIDs)
		merge(entries.filter { captured[$0.inboxID] == revisions[$0.inboxID, default: 0] })
	}

	public func displayName(for inboxID: String,
	                        sender: Bool = false) -> String
	{
		identities[inboxID]?.handle
			.map { "@" + $0 } ?? String(inboxID.prefix(sender ? 8 : 12))
	}
}
