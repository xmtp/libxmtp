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
	// verifies: IDENT-073, IDENT-074, IDENT-075, IDENT-076
	func testPreAuthenticateRunsBeforeSigner() async throws {
		let calls = AuthenticationCalls()
		var options = testOptions()
		options.registration = RegistrationOptions(auto: false)
		options.handlers = ClientHandlers(preAuthenticate: TestPreAuthenticate(calls, fail: false))
		let client = try await SDKClient.create(
			signer: TestRecordingSigner(generateLocalSigner(), calls), options: options,
		)
		XCTAssertEqual(calls.values, [])
		try await client.register()
		XCTAssertEqual(calls.values, ["preAuthenticate", "sign"])
		calls.clear()
		try await client.register()
		XCTAssertEqual(calls.values, [])
		try await client.end()

		options.registration = RegistrationOptions(auto: true)
		options.handlers = ClientHandlers(preAuthenticate: TestPreAuthenticate(calls, fail: true))
		do {
			let unexpected = try await SDKClient.create(
				signer: TestRecordingSigner(generateLocalSigner(), calls), options: options,
			)
			try await unexpected.end()
			XCTFail("A failed preAuthenticate callback allowed registration")
		} catch {}
		XCTAssertEqual(calls.values, ["preAuthenticate"])
	}

	func testConversationCreationRejectsInvalidPeers() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let unregistered = await generateLocalSigner()
		for peer in [client.inboxId(), client.identity().identifier] {
			do {
				_ = try await client.conversations().createDm(peer: peer)
				XCTFail("DM creation accepted an invalid peer")
			} catch {}
		}
		let identity = try await unregistered.identity()
		do {
			_ = try await client.conversations().createDm(peer: identity)
			XCTFail("DM creation accepted an unregistered identity")
		} catch {}
		do {
			_ = try await client.conversations().createGroup(members: [identity])
			XCTFail("Group creation accepted an unregistered identity")
		} catch {}
		try await client.end()
	}

	/// The account-identity overloads in `runtime/IdentityRoutes.swift` make the
	/// same change as their inbox ID forms. Rust `identity_routes.rs` tests the
	/// methods they forward to.
	func testIdentityOverloadsChangeMembership() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let peer = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let conversations = client.conversations()
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
		try await peer.end()
		try await client.end()
	}

	func testStoragePoolOptionsCrossNativeBoundary() async throws {
		for pool in [nil, StoragePoolOptions(min: 1, max: 4), StoragePoolOptions(max: 10)] as [StoragePoolOptions?] {
			var options = testOptions()
			options.storage.pool = pool
			let client = try await SDKClient.create(signer: generateLocalSigner(), options: options)
			XCTAssertEqual(client.options().storage.pool, pool)
			try await client.end()
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

	func testHistorySnapshotKeepsTypedMessagesAndResumesAfterCursor() async throws {
		let codec = SnapshotFailingCodec()
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions(), codecs: [codec])
		let peer = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let group = try await client.conversations().createGroup(members: [InboxId]())
		let first = try await group.sendText(text: "before snapshot")
		let failedId = try await group.send(codec, value: "unreadable custom bytes")
		let snapshot = try await group.messageHistorySnapshot(limit: 100)
		let applications = snapshot.messages.filter { $0.kind == .application }
		XCTAssertEqual(applications.map(\.id), [first, failedId])
		XCTAssertFalse(snapshot.cursor.isEmpty)
		XCTAssertTrue(applications.allSatisfy { $0.deliveryCursor != nil })
		let failed = try XCTUnwrap(applications.first { $0.id == failedId })
		if case let .custom(encoded, bytes, nil, details?) = failed.content {
			XCTAssertEqual(encoded.content, Data("unreadable custom bytes".utf8))
			XCTAssertEqual(encoded.fallback, "Unreadable custom message")
			XCTAssertEqual(bytes, failed.rawBytes)
			XCTAssertFalse(bytes.isEmpty)
			XCTAssertEqual(details.code, "CodecDecodeFailed")
		} else {
			XCTFail("Snapshot lost the custom codec failure")
		}
		let recent = try await group.messageHistorySnapshot(limit: 1)
		XCTAssertEqual(recent.messages.map(\.id), [failedId])
		let after = try await group.sendText(text: "after snapshot")
		let reader = try await group.messageReader(options: ConversationMessageReaderOptions(from: snapshot.cursor))
		let next = try await reader.next()
		XCTAssertEqual(next?.id, after)
		try await reader.end()
		let dm = try await client.conversations().createDm(peer: peer.inboxId())
		let dmId = try await dm.sendText(text: "DM snapshot")
		let dmSnapshot = try await dm.messageHistorySnapshot(limit: 100)
		XCTAssertTrue(dmSnapshot.messages.contains { $0.id == dmId })
		let all = try await client.conversations().messageHistorySnapshot(limit: 100, options: nil)
		XCTAssertTrue(all.messages.contains { $0.id == dmId })
		XCTAssertTrue(all.messages.contains { $0.id == after })
		do {
			_ = try await client.conversations().messageHistorySnapshot(
				limit: 100, options: MessageReaderOptions(from: snapshot.cursor),
			)
			XCTFail("History snapshot accepted a replay-only from option")
		} catch XmtpError.InvalidArgument {}
		try await peer.end()
		try await client.end()
	}

	func testGroupAndDmReadTypedMessages() async throws {
		let sender = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let receiver = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let group = try await sender.conversations().createGroup(members: [receiver.inboxId()], options: nil)
		let dm = try await sender.conversations().createDm(peer: receiver.inboxId(), options: nil)
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

private final class AuthenticationCalls: @unchecked Sendable {
	private let lock = NSLock()
	private var calls: [String] = []
	func append(_ value: String) {
		lock.lock(); defer { lock.unlock() }; calls.append(value)
	}

	func clear() {
		lock.lock(); defer { lock.unlock() }; calls = []
	}

	var values: [String] {
		lock.lock(); defer { lock.unlock() }; return calls
	}
}

private final class TestPreAuthenticate: PreAuthenticate, @unchecked Sendable {
	let calls: AuthenticationCalls
	let fail: Bool
	init(_ calls: AuthenticationCalls, fail: Bool) {
		self.calls = calls; self.fail = fail
	}

	func run() async throws {
		calls.append("preAuthenticate")
		if fail {
			throw PreAuthenticateError.Failed
		}
	}
}

private final class TestRecordingSigner: Signer, @unchecked Sendable {
	let signer: Signer
	let calls: AuthenticationCalls
	init(_ signer: Signer, _ calls: AuthenticationCalls) {
		self.signer = signer; self.calls = calls
	}

	func identity() async throws -> PublicIdentity {
		try await signer.identity()
	}

	func kind() async throws -> SignerKind {
		try await signer.kind()
	}

	func sign(request: SigningRequest) async throws -> Signature {
		calls.append("sign")
		return try await signer.sign(request: request)
	}
}

private struct SnapshotFailingCodec: ContentCodec {
	var type: ContentTypeId {
		ContentTypeId(authorityId: "example.com", typeId: "snapshot-failure", versionMajor: 1, versionMinor: 0)
	}

	func encode(_ value: String) throws -> EncodedContent {
		EncodedContent(type: type, fallback: "Unreadable custom message", content: Data(value.utf8))
	}

	func decode(_: EncodedContent) throws -> String {
		throw SnapshotDecodeError.failed
	}
}

private enum SnapshotDecodeError: Error { case failed }
