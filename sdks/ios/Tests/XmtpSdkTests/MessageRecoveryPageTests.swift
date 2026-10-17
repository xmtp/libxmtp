import XCTest
import XmtpSdk

final class MessageRecoveryPageTests: XCTestCase {
	func testNativeRecordsKeepMessagesAndBothPositionBounds() async throws {
		try await withClients { scope in
			let client = try await scope.create(signer: generateLocalSigner())
			let peer = try await scope.create(signer: generateLocalSigner())
			let group = try await client.conversations.createGroup(members: [InboxId]())
			let dm = try await client.conversations.createDm(peer: peer.inboxId())
			for conversation in [Conversation.group(group: group), .dm(dm: dm)] {
				var ids = [MessageId]()
				for text in ["first", "second", "third"] {
					try await ids.append(conversation.sendText(text: text, options: SendOptions(optimistic: true)))
				}
				let first = try await conversation.messageRecoveryPage(
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
				XCTAssertEqual(lastPosition.sentAt, first.messages.last?.sentAt)
				XCTAssertFalse(lastPosition.messageCursor.isEmpty)
				let next = try await conversation.messageRecoveryPage(
					options: ListMessagesOptions(limit: 2, kind: .application),
					before: nil,
					after: lastPosition,
				)
				XCTAssertEqual(next.messages.map(\.id), Array(ids.suffix(1)))
				XCTAssertFalse(next.hasMore)
				let older = try await conversation.messageRecoveryPage(
					options: ListMessagesOptions(limit: 2, direction: .descending, kind: .application),
					before: next.firstPosition,
					after: nil,
				)
				XCTAssertEqual(older.messages.map(\.id), Array(ids.prefix(2).reversed()))
				try await conversation.publishMessage(id: ids[1])
				let fresh = try await conversation.sendText(text: "after publication", options: SendOptions(optimistic: true))
				let resumed = try await conversation.messageRecoveryPage(options: nil, before: nil, after: lastPosition)
				XCTAssertEqual(resumed.messages.map(\.id), [fresh])
				let defaults: MessageRecoveryPage = switch conversation {
				case let .group(group): try await group.messageRecoveryPage()
				case let .dm(dm): try await dm.messageRecoveryPage()
				}
				XCTAssertEqual(defaults.messages.filter { $0.id == fresh }.map(\.id), [fresh])
			}
		}
	}
}
