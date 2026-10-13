import Foundation

public enum HandleStep {
	/// A failed local read leaves the flag untouched so the next session can try again.
	@MainActor public static func shouldPresent(account: any HandleAccount, completed: Bool) async -> Bool {
		guard !completed else { return false }
		do { return try await account.handles().current == nil } catch { return false }
	}
}
