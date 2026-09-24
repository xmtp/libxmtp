import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

@available(iOS 16, *)
class StreamLifecycleTests: XCTestCase {
	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	/// One-shot catch-up joins a pending group and replays its history from
	/// durable cursors, then stops — and a second call finds nothing owed.
	func testCatchUpToLiveColdCatchesPendingGroupAndIsIdempotent() async throws {
		let fixtures = try await fixtures()

		// bo creates a group with alix and sends. alix has never synced, so both
		// the welcome and the message are owed.
		let boGroup = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.alixClient.inboxID]
		)
		_ = try await boGroup.send(content: "missed while away")

		let before = try await fixtures.alixClient.conversations.listGroups()
		XCTAssertEqual(before.count, 0)

		let summary = try await fixtures.alixClient.catchUpToLive()
		XCTAssertTrue(summary.completed)
		XCTAssertEqual(summary.conversations, 1)
		XCTAssertGreaterThanOrEqual(summary.messages, 1)

		let groups = try await fixtures.alixClient.conversations.listGroups()
		XCTAssertEqual(groups.count, 1)

		// Catch-up delivered the real payload to the local store, not just a
		// counter: the missed text is readable with no further sync.
		let texts: [String] = try await groups[0].messages().compactMap {
			try? $0.content()
		}
		XCTAssertTrue(
			texts.contains("missed while away"),
			"the missed message must be stored and readable after catch-up"
		)

		// Nothing owed now: a second run persists nothing new on either axis.
		let again = try await fixtures.alixClient.catchUpToLive()
		XCTAssertEqual(again.messages, 0)
		XCTAssertEqual(again.conversations, 0)

		try fixtures.cleanUpDatabases()
	}

	/// Backgrounding auto-management is a process-global toggle, on by default.
	func testManageStreamLifecycleDefaultsOn() {
		XCTAssertTrue(Client.manageStreamLifecycle)
	}
}
