import Foundation
import XCTest
import XmtpSdk

private func testOptions() -> ClientOptions {
	ClientOptions(
		backend: .options(options: BackendOptions(
			url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://localhost:5050",
		)),
		storage: StorageOptions(location: .inMemory), deviceSync: false,
	)
}

private func memberIds(_ group: Group) async throws -> Set<InboxId> {
	try await Set(group.members().map(\.inboxId))
}

final class InstalledClientTests: XCTestCase {
	/// The account-identity overloads in `runtime/IdentityRoutes.swift` make the
	/// same change as their inbox ID forms. Rust `identity_routes.rs` tests the
	/// methods they forward to.
	func testIdentityOverloadsChangeMembership() async throws {
		try await withClients { scope in
			let client = try await scope.create(signer: generateLocalSigner(), options: testOptions())
			let peer = try await scope.create(signer: generateLocalSigner(), options: testOptions())
			let conversations = client.conversations
			let me = client.inboxId()
			let other = peer.inboxId()
			let empty = try await conversations.createGroup(members: [PublicIdentity]())
			let alone = try await memberIds(empty)
			XCTAssertEqual(alone, [me])
			let group = try await conversations.createGroup(members: [peer.identity()])
			let created = try await memberIds(group)
			XCTAssertEqual(created, [me, other])
			try await group.removeMembers(members: [peer.identity()])
			let removed = try await memberIds(group)
			XCTAssertEqual(removed, [me])
			let added = try await group.addMembers(members: [peer.identity()])
			XCTAssertEqual(added.added, [other])
			let readded = try await memberIds(group)
			XCTAssertEqual(readded, [me, other])
			let dm = try await conversations.createDm(peer: peer.identity())
			let dmPeer = try await dm.peerInboxId()
			XCTAssertEqual(dmPeer, other)
			let byInbox = try await conversations.createDm(peer: other)
			XCTAssertEqual(byInbox.id(), dm.id())
		}
	}

	/// Smoke for the generated free function: it lowers [InboxId] and BackendSource
	/// and lifts the [String: UInt64] result. No other Swift test lifts that dictionary.
	/// The count is read with `Query`, which reads its own writes (API-202, OPS-009), so no poll is needed.
	func testLatestInboxUpdatesCountWithoutClient() async throws {
		let options = testOptions()
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: options)
		let inboxId = client.inboxId()
		try await client.end()
		let backend = try XCTUnwrap(options.backend)
		let counts = try await latestInboxUpdatesCount(inboxIds: [inboxId], backend: backend)
		XCTAssertEqual(counts, [inboxId: 1])
	}

	func testGroupAndDmReadTypedMessages() async throws {
		let sender = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let receiver = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let group = try await sender.conversations.createGroup(members: [receiver.inboxId()], options: nil)
		let dm = try await sender.conversations.createDm(peer: receiver.inboxId(), options: nil)
		for conversation in [Conversation.group(group: group), .dm(dm: dm)] {
			let sentId = try await conversation.sendText(text: "installed package", options: nil)
			let rows = try await conversation.messages(options: nil)
			let sent = try XCTUnwrap(rows.first { $0.id == sentId })
			guard case let .standard(.text(text)) = sent.content else { return XCTFail("Text was not typed") }
			XCTAssertEqual(text, "installed package")
			XCTAssertEqual(sent.senderInboxId, sender.inboxId())
			XCTAssertEqual(sent.conversationId, conversation.id())
			// Smoke for the host-only Message actions (SDKTypes.swift). Rust tests
			// cover the core behavior, not these Swift forwards.
			let replyId = try await sent.reply("reply")
			let reactionId = try await sent.react(Reaction(content: "+1", action: .added, schema: .shortcode))
			let replies = try await conversation.messages(options: nil)
			let reply = try XCTUnwrap(replies.first { $0.id == replyId })
			let parent = try await reply.parent()
			XCTAssertEqual(parent?.id, sentId)
			let refreshed = try await sent.refresh()
			let related = try XCTUnwrap(refreshed)
			XCTAssertEqual(related.replyCount, 1)
			XCTAssertEqual(related.reactions.map(\.id), [reactionId])
			_ = try await sent.delete()
			let afterDelete = try await sent.refresh()
			let deleted = try XCTUnwrap(afterDelete)
			guard case .standard(.deletedMessage) = deleted.content else {
				return XCTFail("Message.delete did not delete the message")
			}
		}
		try await receiver.end()
		try await sender.end()
	}
}
