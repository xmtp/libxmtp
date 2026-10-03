import Foundation
import XCTest
import XmtpSdk

private func testOptions(location: StorageLocation = .inMemory) -> ClientOptions {
	ClientOptions(
		backend: .options(options: BackendOptions(
			url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://localhost:5050",
		)),
		storage: StorageOptions(location: location), deviceSync: false,
	)
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

	func testOptimisticGroupPublishesPreparedMessage() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let peer = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let group = try await client.conversations().createGroupOptimistic(options: CreateGroupOptions(name: "Testing"))
		let prepared = try await group.prepareMessage(TextCodec(), value: "prepared text")
		let before = try await group.messages(options: nil)
		XCTAssertTrue(before.contains { $0.id == prepared })
		_ = try await group.addMembers(members: [peer.inboxId()])
		try await group.sync()
		try await group.publishMessages()
		let after = try await group.messages(options: nil)
		XCTAssertTrue(after.contains {
			if case let .standard(.text(body)) = $0.content {
				return $0.id == prepared && body == "prepared text"
			}
			return false
		})
		let members = try await group.members()
		XCTAssertEqual(Set(members.map(\.inboxId)), [client.inboxId(), peer.inboxId()])
		let state = try await group.state()
		XCTAssertEqual(state.name, "Testing")
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

	func testEncryptedStoreReopensAtSamePath() async throws {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		defer { try? FileManager.default.removeItem(at: root) }
		let signer = await generateLocalSigner()
		var options = testOptions(location: .directory(directory: root.path))
		options.storage.encryptionKey = Data(repeating: 42, count: 32)
		let client = try await SDKClient.create(signer: signer, options: options)
		let path = try await client.storage().path()
		let inboxId = client.inboxId()
		let identity = client.identity()
		try await client.end()
		let reopened = try await SDKClient.build(identity: identity, options: options)
		XCTAssertEqual(reopened.inboxId(), inboxId)
		let reopenedPath = try await reopened.storage().path()
		XCTAssertEqual(reopenedPath, path)
		try await reopened.end()
		options.storage.encryptionKey = Data(repeating: 43, count: 32)
		do {
			let unexpected = try await SDKClient.build(identity: identity, options: options)
			try await unexpected.end()
			XCTFail("The wrong encryption key opened the database")
		} catch {}
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

	func testHistorySnapshotsKeepRelationsAndHideDeletedContent() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let peer = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let group = try await client.conversations().createGroup(members: [InboxId]())
		let dm = try await client.conversations().createDm(peer: peer.inboxId())
		for conversation in [Conversation.group(group: group), .dm(dm: dm)] {
			let originalText = "private original \(UUID().uuidString)"
			let originalFallback = "private fallback \(UUID().uuidString)"
			var encoded = try encodeText(text: originalText)
			encoded.fallback = originalFallback
			let originalId = try await conversation.send(encoded: encoded, options: nil)
			let originalRows = try await conversation.messages(options: nil)
			let original = try XCTUnwrap(originalRows.first { $0.id == originalId })
			XCTAssertEqual(original.fallback, originalFallback)
			let replyId = try await original.reply("public reply")
			let reactionId = try await original.react(Reaction(content: "+1", action: .added, schema: .shortcode))
			let ordinary = try await conversation.messages(options: nil)
			let ordinaryParent = try XCTUnwrap(ordinary.first { $0.id == originalId })
			XCTAssertEqual(ordinaryParent.replyCount, 1)
			XCTAssertEqual(ordinaryParent.reactions.map(\.id), [reactionId])
			for snapshot in try await [
				conversation.messageHistorySnapshot(limit: 100),
				client.conversations().messageHistorySnapshot(limit: 100, options: nil),
			] {
				let parent = try XCTUnwrap(snapshot.messages.first { $0.id == originalId })
				let reply = try XCTUnwrap(snapshot.messages.first { $0.id == replyId })
				XCTAssertEqual(parent.replyCount, ordinaryParent.replyCount)
				XCTAssertEqual(parent.reactions, ordinaryParent.reactions)
				XCTAssertEqual(reply.inReplyTo?.id, originalId)
				XCTAssertFalse(snapshot.cursor.isEmpty)
				XCTAssertNotNil(parent.deliveryCursor)
				XCTAssertNotNil(reply.deliveryCursor)
			}
			let recent = try await conversation.messageHistorySnapshot(limit: 2)
			XCTAssertEqual(recent.messages.map(\.id), [replyId, reactionId])
			_ = try await original.delete()
			for snapshot in try await [
				conversation.messageHistorySnapshot(limit: 100),
				client.conversations().messageHistorySnapshot(limit: 100, options: nil),
			] {
				try assertDeletedSnapshot(snapshot, originalId: originalId, replyId: replyId)
			}
		}
		try await peer.end()
		try await client.end()
	}

	private func assertDeletedSnapshot(
		_ snapshot: MessageHistorySnapshot, originalId: MessageId, replyId: MessageId,
	) throws {
		let deleted = try XCTUnwrap(snapshot.messages.first { $0.id == originalId })
		switch deleted.content {
		case .standard(.deletedMessage(_)): break
		default:
			XCTFail("History snapshot disclosed the original message after deletion")
		}
		XCTAssertTrue(deleted.rawBytes.isEmpty)
		XCTAssertTrue(deleted.encoded?.content.isEmpty ?? true)
		XCTAssertNil(deleted.fallback)
		XCTAssertNotNil(deleted.deliveryCursor)
		let reply = try XCTUnwrap(snapshot.messages.first { $0.id == replyId })
		if let parent = reply.inReplyTo {
			switch parent.content {
			case .deletedMessage: break
			default:
				XCTFail("History snapshot disclosed the deleted reply parent")
			}
			XCTAssertTrue(parent.rawBytes.isEmpty)
			XCTAssertTrue(parent.encoded?.content.isEmpty ?? true)
			XCTAssertNil(parent.fallback)
		} else {
			XCTFail("History snapshot lost the deleted reply parent")
		}
		XCTAssertFalse(snapshot.cursor.isEmpty)
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
		}
		try await receiver.end()
		try await sender.end()
	}

	func testArchiveImportIntoPopulatedStoreKeepsMessages() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let group = try await client.conversations().createGroup(members: [String](), options: nil)
		let first = try await group.sendText(text: "before archive", options: nil)
		let key = Data(repeating: 7, count: 32)
		let archive = try await client.archives().exportToBytes(keyBytes: key, options: nil)
		let second = try await group.sendText(text: "after archive", options: nil)
		try await client.archives().importFromBytes(data: archive, keyBytes: key)
		let third = try await group.sendText(text: "after import", options: nil)
		let rows = try await group.messages(options: nil)
		XCTAssertEqual(rows.filter { [first, second, third].contains($0.id) }.count, 3)
		let listed = try await client.conversations().list()
		XCTAssertEqual(listed.filter { $0.id() == group.id() }.count, 1)
		try await client.end()
	}

	func testLeftInboxesPersistAfterReopen() async throws {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		defer { try? FileManager.default.removeItem(at: root) }
		let options = testOptions(location: .directory(directory: root.path))
		let owner = try await SDKClient.create(signer: generateLocalSigner(), options: options)
		let peer = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let group = try await owner.conversations().createGroup(members: [peer.inboxId()], options: nil)
		_ = try await peer.conversations().syncAll(consentStates: nil)
		let joined = try await peer.conversations().getById(id: group.id())
		guard case let .group(peerGroup)? = joined else { return XCTFail("Peer did not join the group") }
		try await peerGroup.requestRemoval()
		let deadline = Date().addingTimeInterval(15)
		var left: Message?
		repeat {
			try await group.sync()
			left = try await group.messages(options: nil).first {
				if case let .standard(.groupUpdated(update)) = $0.content {
					return update.leftInboxes.contains(peer.inboxId())
				}
				return false
			}
			if left == nil {
				try await Task.sleep(nanoseconds: 100_000_000)
			}
		} while left == nil && Date() < deadline
		let retained = try XCTUnwrap(left)
		let identity = owner.identity()
		let groupId = group.id()
		let peerId = peer.inboxId()
		try await owner.end()
		try await peer.end()
		let reopened = try await SDKClient.build(identity: identity, options: options)
		let restored = try await reopened.conversations().getById(id: groupId)
		let rows = try await restored?.messages(options: nil)
		let persisted = try XCTUnwrap(rows?.first { $0.id == retained.id })
		guard case let .standard(.groupUpdated(update)) = persisted.content else {
			return XCTFail("Stored group update lost its typed content")
		}
		XCTAssertEqual(update.leftInboxes, [peerId])
		XCTAssertTrue(update.removedInboxes.isEmpty)
		try await reopened.end()
	}

	func testHistoryPaginationAndPerformance() async throws {
		try XCTSkipUnless(ProcessInfo.processInfo.environment["XMTP_IOS_PERFORMANCE"] == "1")
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let group = try await client.conversations().createGroup(members: [String](), options: nil)
		for index in 0 ..< 30 {
			_ = try await group.sendText(text: "Message \(index)", options: nil)
		}
		let started = Date()
		let rows = try await group.messages(options: nil)
		let elapsed = Date().timeIntervalSince(started)
		XCTAssertGreaterThanOrEqual(rows.count, 30)
		XCTAssertLessThan(elapsed, 2)
		print("ios-history rows=\(rows.count) seconds=\(elapsed)")
		let first = try await group.messages(options: ListMessagesOptions(limit: 10, direction: .descending))
		let oldest = try XCTUnwrap(first.last)
		let second = try await group.messages(options: ListMessagesOptions(
			limit: 10,
			sentBefore: oldest.sentAt,
			direction: .descending,
		))
		XCTAssertEqual(first.count, 10)
		XCTAssertFalse(second.isEmpty)
		XCTAssertTrue(Set(first.map(\.id)).isDisjoint(with: Set(second.map(\.id))))
		try await client.end()
	}

	func testManyGroupSyncAndParallelMembers() async throws {
		try XCTSkipUnless(ProcessInfo.processInfo.environment["XMTP_IOS_PERFORMANCE"] == "1")
		let owner = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		let peer = try await SDKClient.create(signer: generateLocalSigner(), options: testOptions())
		var groups: [XmtpSdk.Group] = []
		for _ in 0 ..< 100 {
			try await groups.append(owner.conversations().createGroup(members: [peer.inboxId()], options: nil))
		}
		let syncStart = Date()
		let summary = try await peer.conversations().syncAll(consentStates: nil)
		let syncSeconds = Date().timeIntervalSince(syncStart)
		XCTAssertEqual(summary.eligible, 100)
		XCTAssertLessThan(syncSeconds, 60)
		let membersStart = Date()
		let counts = try await withThrowingTaskGroup(of: Int.self, returning: [Int].self) { tasks in
			for group in groups {
				tasks.addTask { try await group.members().count }
			}
			var result: [Int] = []
			for try await count in tasks {
				result.append(count)
			}
			return result
		}
		let membersSeconds = Date().timeIntervalSince(membersStart)
		XCTAssertEqual(counts.count, 100)
		XCTAssertTrue(counts.allSatisfy { $0 == 2 })
		XCTAssertLessThan(membersSeconds, 10)
		print("ios-groups rows=100 syncSeconds=\(syncSeconds) membersSeconds=\(membersSeconds)")
		for group in groups {
			try await group.removeMembers(members: [peer.inboxId()])
		}
		let removals = try await peer.conversations().syncAll(consentStates: nil)
		XCTAssertEqual(removals.eligible, 100)
		let stored = try await peer.conversations().listGroups(options: nil)
		XCTAssertEqual(stored.count, 100)
		for group in stored {
			let state = try await group.state(); XCTAssertFalse(state.common.isActive)
		}
		try await owner.end()
		try await peer.end()
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
