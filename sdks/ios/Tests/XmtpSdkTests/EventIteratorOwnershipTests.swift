import Foundation
import XCTest
import XmtpSdk

final class EventIteratorOwnershipTests: XCTestCase {
	func testSecondIteratorCannotConsumeOrCloseFirstIterator() async throws {
		let options = ClientOptions(
			backend: .options(options: BackendOptions(
				url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://localhost:5050",
			)),
			storage: StorageOptions(location: .inMemory), deviceSync: false,
		)
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: options)
		let stream = try await client.events(EventFilter(
			kinds: [.conversationJoined], conversationIds: nil,
			contentTypes: nil, referencesOwnMessages: true,
		))
		let first = stream.makeAsyncIterator()
		var second: SDKEventStream.Iterator? = stream.makeAsyncIterator()
		let group = try await client.conversations().createGroup(members: [InboxId]())
		var rejected = false
		do {
			_ = try await second?.next()
			XCTFail("A second iterator consumed the first iterator's event")
		} catch let XmtpError.ConsumerOwned(details) {
			rejected = true
			XCTAssertEqual(details.code, "ConsumerOwned")
			XCTAssertEqual(details.category, .stream)
		}
		second = nil
		guard rejected else {
			try await client.end()
			return
		}
		let another = try await client.conversations().createGroup(members: [InboxId]())
		let firstValue = try await first.next()
		let secondValue = try await first.next()
		guard case let .conversationJoined(conversationJoined: firstJoined)? = firstValue,
		      case let .conversationJoined(conversationJoined: secondJoined)? = secondValue
		else {
			try await client.end()
			return XCTFail("Dropping a rejected iterator closed the active event reader")
		}
		XCTAssertEqual(firstJoined.conversationId, group.id())
		XCTAssertEqual(secondJoined.conversationId, another.id())
		try await client.end()
	}
}
