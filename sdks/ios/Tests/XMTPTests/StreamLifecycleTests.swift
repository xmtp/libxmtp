import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

@available(iOS 16, *)
class StreamLifecycleTests: XCTestCase {
	/// One-shot catch-up joins a pending group and replays its history from
	/// durable cursors, then stops — and a second call finds nothing owed.
	func testCatchUpToLiveColdCatchesPendingGroupAndIsIdempotent() async throws {
		let fixtures = try await fixtures()
		let receiver = try await Client.create(
			account: PrivateKey.generate(),
			options: ClientOptions(
				api: localApi(),
				dbEncryptionKey: Data((0 ..< 32).map { _ in UInt8.random(in: 0 ... 255) }),
				deviceSyncEnabled: false
			)
		)

		// Disable the receiver's device-sync worker so no background Welcome receipt runs.
		// The Welcome and application message must both wait for catch-up.
		let boGroup = try await fixtures.boClient.conversations.newGroup(
			with: [receiver.inboxID]
		)
		let messageID = try await boGroup.send(content: "missed while away")

		let before = try await receiver.conversations.listGroups()
		XCTAssertEqual(before.count, 0)

		let summary = try await receiver.catchUpToLive()
		XCTAssertTrue(summary.completed)
		XCTAssertEqual(summary.failed, 0)
		XCTAssertEqual(summary.conversations, 1)
		XCTAssertGreaterThanOrEqual(summary.messages, 1)

		let groups = try await receiver.conversations.listGroups()
		XCTAssertEqual(groups.count, 1)

		// Catch-up delivered the real payload to the local store, not just a
		// counter: the missed text is readable with no further sync.
		let group = try XCTUnwrap(groups.first)
		XCTAssertEqual(group.id, boGroup.id)
		let applications = try await group.messages().filter { $0.kind == .application }
		XCTAssertEqual(applications.map(\.id), [messageID])
		let message = try XCTUnwrap(applications.first)
		let text: String = try message.content()
		XCTAssertEqual(text, "missed while away")
		XCTAssertEqual(message.senderInboxId, fixtures.boClient.inboxID)

		// Nothing owed now: a second run persists nothing new on either axis.
		let again = try await receiver.catchUpToLive()
		XCTAssertTrue(again.completed)
		XCTAssertEqual(again.failed, 0)
		XCTAssertEqual(again.messages, 0)
		XCTAssertEqual(again.conversations, 0)

		try receiver.deleteLocalDatabase()
		try fixtures.cleanUpDatabases()
	}

	/// Backgrounding auto-management is a process-global toggle, on by default.
	func testManageStreamLifecycleDefaultsOn() {
		XCTAssertTrue(Client.manageStreamLifecycle)
	}
}
