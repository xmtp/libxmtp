import Foundation

/// Removes a notification that arrives after a privacy change crossed its submission.
@MainActor public final class PushSubmissionFence {
	public private(set) var revision: UInt64 = 0
	public init() {}
	public func invalidate() {
		revision &+= 1
	}

	public func submit(
		expectedRevision: UInt64,
		add: @MainActor () async throws -> Void,
		remove: @MainActor () -> Void,
	) async -> Bool {
		guard revision == expectedRevision else { return false }
		do { try await add() } catch {
			if revision != expectedRevision {
				remove()
			}
			return false
		}
		guard revision == expectedRevision else { remove(); return false }
		return true
	}
}
