import XCTest
import XmtpSdk

final class MessageHistoryPageTests: XCTestCase {
	func testNativeRecordsKeepMessagesAndBothPositionBounds() async throws {
		try await withClients { scope in
			let client = try await scope.create(signer: generateLocalSigner())
			let peer = try await scope.create(signer: generateLocalSigner())
			let group = try await client.conversations.createGroup(members: [InboxId]())
			let dm = try await client.conversations.createDm(peer: peer.inboxId())
			for conversation in [Conversation.group(group: group), .dm(dm: dm)] {
				var ids = [MessageId]()
				for text in ["first", "second", "third"] {
					try await ids.append(conversation.sendText(text: text, options: nil))
				}
				let first = try await conversation.messageHistoryPage(
					options: ListMessagesOptions(limit: 2, kind: .application),
					before: nil,
					after: nil,
				)
				XCTAssertEqual(first.messages.map(\.id), Array(ids.prefix(2)))
				XCTAssertEqual(first.skippedCount, 0)
				XCTAssertTrue(first.hasMore)
				let firstPosition = try XCTUnwrap(first.firstPosition)
				let lastPosition = try XCTUnwrap(first.lastPosition)
				XCTAssertEqual(firstPosition.sentAt, first.messages.first?.sentAt)
				XCTAssertEqual(lastPosition.deliveryCursor, first.messages.last?.deliveryCursor)
				let next = try await conversation.messageHistoryPage(
					options: ListMessagesOptions(limit: 2, kind: .application),
					before: nil,
					after: lastPosition,
				)
				XCTAssertEqual(next.messages.map(\.id), Array(ids.suffix(1)))
				XCTAssertFalse(next.hasMore)
				let older = try await conversation.messageHistoryPage(
					options: ListMessagesOptions(limit: 2, direction: .descending, kind: .application),
					before: next.firstPosition,
					after: nil,
				)
				XCTAssertEqual(older.messages.map(\.id), Array(ids.prefix(2).reversed()))
				let defaults: MessageHistoryPage = switch conversation {
				case let .group(group): try await group.messageHistoryPage()
				case let .dm(dm): try await dm.messageHistoryPage()
				}
				XCTAssertEqual(defaults.messages.filter { ids.contains($0.id) }.map(\.id), ids)
			}
		}
	}
}
