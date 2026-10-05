import Foundation
import XCTest
import XmtpSdk

/// The Swift listener wrapper (`runtime/events/SDKEvents.swift`) on a live client.
final class EventListenerTests: XCTestCase {
	func testListenerRunsUntilItStops() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: liveOptions())
		let calls = Shared(0)
		let filter = EventFilter(
			kinds: [.conversationJoined], groupIds: nil, contentTypes: nil, referencesOwnMessages: false,
		)
		let id = try await client.startListener(filter) { _ in calls.update { $0 += 1 } }
		_ = try await client.conversations().createGroup(members: [InboxId]())
		let called = await eventually(seconds: 10) { calls.value == 1 }
		XCTAssertTrue(called, "The listener did not run for the event")

		await client.stopListener(id)
		_ = try await client.conversations().createGroup(members: [InboxId]())
		try await Task.sleep(for: .milliseconds(500))
		XCTAssertEqual(calls.value, 1, "A stopped listener ran")
		try await client.end()
	}
}
