import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

/// Reproduces message loss caused by the stream's internal recovery mechanism
/// racing with concurrent sync operations during rapid group membership changes.
///
/// When process_message fails in the stream (e.g., due to epoch transitions from
/// member additions), the stream's factory calls process_or_recover, which falls
/// back to sync_with_conn. This sync advances the cursor (trust_message_order=true),
/// consuming messages that the stream hasn't delivered yet.
///
/// The messages exist in the libxmtp database after a manual sync, but the stream
/// never yields them to the app — so they never appear in the UI.
@available(iOS 16, *)
class ForwardSecrecyReproTests: XCTestCase {
	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	// MARK: - Reproduction tests

	/// Reproduces message loss when the stream callback blocks on heavy processing.
	///
	/// Simulates the Convos iOS app pipeline where each streamed message triggers:
	/// - findConversation (network call)
	/// - conversation.sync() (the primary cause — calls sync_with_conn)
	/// - member/permission fetches
	/// - database writes (~50ms latency)
	///
	/// While the stream callback is blocked on these operations, the stream's
	/// internal recovery mechanism processes subsequent messages via sync_with_conn,
	/// advancing the cursor past them. When the callback returns and the stream
	/// tries to deliver the next message, it's already been consumed.
	///
	/// Expected: fails with ~50% message loss (103/200 in testing).
	func testBlockingStreamCallbackWithSyncLosesMessages() async throws {
		try TestConfig.skipIfNotRunningLocalNodeTests()

		let fixtures = try await fixtures()

		let alixGroup = try await fixtures.alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID]
		)
		try await fixtures.boClient.conversations.sync()
		let boGroup = try await fixtures.boClient.conversations.findGroup(
			groupId: alixGroup.id
		)!

		let receivedMessages = TestTranscript()
		let streamTask = Task(priority: .userInitiated) {
			for try await message in boGroup.streamMessages() {
				// Simulate the Convos app's StreamProcessor.processMessage pipeline
				let _ = try? await fixtures.boClient.conversations.findConversation(
					conversationId: message.conversationId
				)
				try? await boGroup.sync()
				let _ = try? await boGroup.members
				let _ = try? boGroup.permissionPolicySet()
				try? await Task.sleep(nanoseconds: 50_000_000)

				if (try? message.encodedContent.type == ContentTypeText) ?? false {
					let text = (try? message.content() as String) ?? ""
					await receivedMessages.add(text)
				}
			}
		}

		try await Task.sleep(nanoseconds: 1_000_000_000)

		let messageCount = 200

		let sendTask = Task {
			for i in 1...messageCount {
				_ = try? await alixGroup.send(content: "msg-\(i)")
			}
		}

		let joinTask = Task {
			let key = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
			let opts = ClientOptions(
				api: ClientOptions.Api(
					env: XMTPEnvironment.local,
					isSecure: XMTPEnvironment.local.isSecure
				),
				dbEncryptionKey: key
			)
			for _ in 1...4 {
				let newKey = try PrivateKey.generate()
				let newClient = try await Client.create(account: newKey, options: opts)
				_ = try? await alixGroup.addMembers(inboxIds: [newClient.inboxID])
			}
		}

		let syncTask = Task {
			for _ in 1...20 {
				_ = try? await fixtures.boClient.conversations.syncAllConversations()
				try? await Task.sleep(nanoseconds: 50_000_000)
			}
		}

		_ = await sendTask.result
		_ = await joinTask.result
		_ = await syncTask.result
		try await Task.sleep(nanoseconds: 5_000_000_000)

		try await boGroup.sync()
		let allMessages = try await boGroup.messages()
		let textMessages = allMessages.filter { (try? $0.encodedContent.type == ContentTypeText) ?? false }

		streamTask.cancel()

		let streamCount = await receivedMessages.messages.count

		print("=== Blocking Stream Callback With Sync ===")
		print("Messages sent: \(messageCount)")
		print("Messages via stream: \(streamCount)")
		print("Messages in DB: \(textMessages.count)")

		XCTAssertEqual(
			streamCount, messageCount,
			"Stream delivered only \(streamCount)/\(messageCount) messages. " +
			"\(messageCount - streamCount) lost because sync_with_conn in the " +
			"stream callback advances the cursor past undelivered messages."
		)

		try fixtures.cleanUpDatabases()
	}

	/// Same scenario but without conversation.sync() in the stream callback.
	///
	/// Removing sync() from the callback reduces loss but doesn't eliminate it,
	/// because the stream callback still blocks on findConversation, members,
	/// and permissions — giving the stream's internal recovery mechanism time
	/// to race ahead.
	///
	/// Expected: fails with ~45% message loss (109/200 in testing).
	func testBlockingStreamCallbackWithoutSyncStillLosesMessages() async throws {
		try TestConfig.skipIfNotRunningLocalNodeTests()

		let fixtures = try await fixtures()

		let alixGroup = try await fixtures.alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID]
		)
		try await fixtures.boClient.conversations.sync()
		let boGroup = try await fixtures.boClient.conversations.findGroup(
			groupId: alixGroup.id
		)!

		let receivedMessages = TestTranscript()
		let streamTask = Task(priority: .userInitiated) {
			for try await message in boGroup.streamMessages() {
				// Same pipeline but without conversation.sync()
				let _ = try? await fixtures.boClient.conversations.findConversation(
					conversationId: message.conversationId
				)
				// sync() removed — this is the skipSync fix
				let _ = try? await boGroup.members
				let _ = try? boGroup.permissionPolicySet()
				try? await Task.sleep(nanoseconds: 50_000_000)

				if (try? message.encodedContent.type == ContentTypeText) ?? false {
					let text = (try? message.content() as String) ?? ""
					await receivedMessages.add(text)
				}
			}
		}

		try await Task.sleep(nanoseconds: 1_000_000_000)

		let messageCount = 200

		let sendTask = Task {
			for i in 1...messageCount {
				_ = try? await alixGroup.send(content: "msg-\(i)")
			}
		}

		let joinTask = Task {
			let key = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
			let opts = ClientOptions(
				api: ClientOptions.Api(
					env: XMTPEnvironment.local,
					isSecure: XMTPEnvironment.local.isSecure
				),
				dbEncryptionKey: key
			)
			for _ in 1...4 {
				let newKey = try PrivateKey.generate()
				let newClient = try await Client.create(account: newKey, options: opts)
				_ = try? await alixGroup.addMembers(inboxIds: [newClient.inboxID])
			}
		}

		let syncTask = Task {
			for _ in 1...20 {
				_ = try? await fixtures.boClient.conversations.syncAllConversations()
				try? await Task.sleep(nanoseconds: 50_000_000)
			}
		}

		_ = await sendTask.result
		_ = await joinTask.result
		_ = await syncTask.result
		try await Task.sleep(nanoseconds: 5_000_000_000)

		try await boGroup.sync()
		let allMessages = try await boGroup.messages()
		let textMessages = allMessages.filter { (try? $0.encodedContent.type == ContentTypeText) ?? false }

		streamTask.cancel()

		let streamCount = await receivedMessages.messages.count

		print("=== Blocking Stream Callback Without Sync ===")
		print("Messages sent: \(messageCount)")
		print("Messages via stream: \(streamCount)")
		print("Messages in DB: \(textMessages.count)")

		XCTAssertEqual(
			streamCount, messageCount,
			"Stream delivered only \(streamCount)/\(messageCount) messages. " +
			"Even without sync() in the callback, blocking the stream iteration " +
			"allows the recovery mechanism to consume messages."
		)

		try fixtures.cleanUpDatabases()
	}

	// MARK: - Fix verification

	/// Non-blocking stream callback delivers all messages.
	///
	/// When messages are dispatched to a Task instead of being processed inline,
	/// the stream iteration is never blocked. The for-await loop consumes messages
	/// as fast as libxmtp delivers them, preventing the recovery mechanism from
	/// racing ahead.
	///
	/// Expected: passes with 200/200 messages.
	func testNonBlockingStreamCallbackDoesNotLoseMessages() async throws {
		try TestConfig.skipIfNotRunningLocalNodeTests()

		let fixtures = try await fixtures()

		let alixGroup = try await fixtures.alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID]
		)
		try await fixtures.boClient.conversations.sync()
		let boGroup = try await fixtures.boClient.conversations.findGroup(
			groupId: alixGroup.id
		)!

		let receivedMessages = TestTranscript()
		let streamTask = Task(priority: .userInitiated) {
			for try await message in boGroup.streamMessages() {
				let capturedMessage = message
				Task {
					let _ = try? await fixtures.boClient.conversations.findConversation(
						conversationId: capturedMessage.conversationId
					)
					let _ = try? await boGroup.members
					let _ = try? boGroup.permissionPolicySet()
					try? await Task.sleep(nanoseconds: 50_000_000)

					if (try? capturedMessage.encodedContent.type == ContentTypeText) ?? false {
						let text = (try? capturedMessage.content() as String) ?? ""
						await receivedMessages.add(text)
					}
				}
			}
		}

		try await Task.sleep(nanoseconds: 1_000_000_000)

		let messageCount = 200

		let sendTask = Task {
			for i in 1...messageCount {
				_ = try? await alixGroup.send(content: "msg-\(i)")
			}
		}

		let joinTask = Task {
			let key = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
			let opts = ClientOptions(
				api: ClientOptions.Api(
					env: XMTPEnvironment.local,
					isSecure: XMTPEnvironment.local.isSecure
				),
				dbEncryptionKey: key
			)
			for _ in 1...4 {
				let newKey = try PrivateKey.generate()
				let newClient = try await Client.create(account: newKey, options: opts)
				_ = try? await alixGroup.addMembers(inboxIds: [newClient.inboxID])
			}
		}

		let syncTask = Task {
			for _ in 1...20 {
				_ = try? await fixtures.boClient.conversations.syncAllConversations()
				try? await Task.sleep(nanoseconds: 50_000_000)
			}
		}

		_ = await sendTask.result
		_ = await joinTask.result
		_ = await syncTask.result
		try await Task.sleep(nanoseconds: 10_000_000_000)

		streamTask.cancel()

		let streamCount = await receivedMessages.messages.count

		print("=== Non-Blocking Stream Callback ===")
		print("Messages sent: \(messageCount)")
		print("Messages via stream: \(streamCount)")

		XCTAssertEqual(
			streamCount, messageCount,
			"Expected \(messageCount) messages, got \(streamCount)."
		)

		try fixtures.cleanUpDatabases()
	}

	// MARK: - Control tests

	/// Control: stream-only (no concurrent sync) does not lose messages.
	func testStreamOnlyDoesNotLoseMessages() async throws {
		try TestConfig.skipIfNotRunningLocalNodeTests()

		let fixtures = try await fixtures()

		let alixGroup = try await fixtures.alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID]
		)
		try await fixtures.boClient.conversations.sync()
		let boGroup = try await fixtures.boClient.conversations.findGroup(
			groupId: alixGroup.id
		)!

		let receivedMessages = TestTranscript()
		let streamTask = Task(priority: .userInitiated) {
			for try await message in boGroup.streamMessages() {
				if (try? message.encodedContent.type == ContentTypeText) ?? false {
					let text = (try? message.content() as String) ?? ""
					await receivedMessages.add(text)
				}
			}
		}

		try await Task.sleep(nanoseconds: 1_000_000_000)

		let messageCount = 200

		let sendTask = Task {
			for i in 1...messageCount {
				_ = try? await alixGroup.send(content: "msg-\(i)")
			}
		}

		let joinTask = Task {
			let key = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
			let opts = ClientOptions(
				api: ClientOptions.Api(
					env: XMTPEnvironment.local,
					isSecure: XMTPEnvironment.local.isSecure
				),
				dbEncryptionKey: key
			)
			for _ in 1...4 {
				let newKey = try PrivateKey.generate()
				let newClient = try await Client.create(account: newKey, options: opts)
				_ = try? await alixGroup.addMembers(inboxIds: [newClient.inboxID])
			}
		}

		_ = await sendTask.result
		_ = await joinTask.result
		try await Task.sleep(nanoseconds: 3_000_000_000)

		try await boGroup.sync()
		let allMessages = try await boGroup.messages()
		let textMessages = allMessages.filter { (try? $0.encodedContent.type == ContentTypeText) ?? false }

		streamTask.cancel()

		print("=== Stream Only (no concurrent sync) ===")
		print("Messages sent: \(messageCount)")
		print("Messages in DB: \(textMessages.count)")

		XCTAssertEqual(
			textMessages.count, messageCount,
			"Expected \(messageCount) messages, got \(textMessages.count)."
		)

		try fixtures.cleanUpDatabases()
	}

	/// Control: sync-only (no stream) does not lose messages.
	func testSyncOnlyDoesNotLoseMessages() async throws {
		try TestConfig.skipIfNotRunningLocalNodeTests()

		let fixtures = try await fixtures()

		let alixGroup = try await fixtures.alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID]
		)
		try await fixtures.boClient.conversations.sync()

		let messageCount = 200

		let sendTask = Task {
			for i in 1...messageCount {
				_ = try? await alixGroup.send(content: "msg-\(i)")
			}
		}

		let joinTask = Task {
			let key = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
			let opts = ClientOptions(
				api: ClientOptions.Api(
					env: XMTPEnvironment.local,
					isSecure: XMTPEnvironment.local.isSecure
				),
				dbEncryptionKey: key
			)
			for _ in 1...4 {
				let newKey = try PrivateKey.generate()
				let newClient = try await Client.create(account: newKey, options: opts)
				_ = try? await alixGroup.addMembers(inboxIds: [newClient.inboxID])
			}
		}

		_ = await sendTask.result
		_ = await joinTask.result
		try await Task.sleep(nanoseconds: 2_000_000_000)

		_ = try await fixtures.boClient.conversations.syncAllConversations()
		let boGroup = try await fixtures.boClient.conversations.findGroup(
			groupId: alixGroup.id
		)!
		try await boGroup.sync()
		let allMessages = try await boGroup.messages()
		let textMessages = allMessages.filter { (try? $0.encodedContent.type == ContentTypeText) ?? false }

		print("=== Sync Only (no stream) ===")
		print("Messages sent: \(messageCount)")
		print("Messages in DB: \(textMessages.count)")

		XCTAssertEqual(
			textMessages.count, messageCount,
			"Expected \(messageCount) messages, got \(textMessages.count)."
		)

		try fixtures.cleanUpDatabases()
	}

	// MARK: - Additional investigation tests

	/// Test: streamAllMessages across multiple groups with simultaneous activity.
	///
	/// The Convos app uses streamAllMessages (not per-group streams) to receive
	/// messages for all conversations. If multiple groups receive rapid messages
	/// simultaneously, processing one group's messages could cause the stream
	/// to miss messages from another group.
	func testStreamAllMessagesAcrossMultipleGroupsDoesNotLoseMessages() async throws {
		try TestConfig.skipIfNotRunningLocalNodeTests()

		let apiOpts = ClientOptions.Api(
			env: XMTPEnvironment.local,
			isSecure: XMTPEnvironment.local.isSecure
		)

		let alixKey = try PrivateKey.generate()
		let alixDbKey = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
		let alixClient = try await Client.create(
			account: alixKey,
			options: ClientOptions(api: apiOpts, dbEncryptionKey: alixDbKey)
		)

		let boKey = try PrivateKey.generate()
		let boDbKey = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
		let boClient = try await Client.create(
			account: boKey,
			options: ClientOptions(api: apiOpts, dbEncryptionKey: boDbKey)
		)

		// Create 5 groups, all with bo as a member
		var groups: [XMTPiOS.Group] = []
		for _ in 0..<5 {
			let group = try await alixClient.conversations.newGroup(
				with: [boClient.inboxID]
			)
			groups.append(group)
		}

		try await boClient.conversations.sync()

		// Bo uses streamAllMessages (like the real app)
		let receivedByGroup = TestGroupTranscript(groupCount: groups.count)
		let streamTask = Task(priority: .userInitiated) {
			for try await message in boClient.conversations.streamAllMessages() {
				// Non-blocking dispatch (our fix)
				let capturedMessage = message
				Task {
					if (try? capturedMessage.encodedContent.type == ContentTypeText) ?? false {
						let text = (try? capturedMessage.content() as String) ?? ""
						await receivedByGroup.add(
							text,
							groupId: capturedMessage.conversationId
						)
					}
				}
			}
		}

		try await Task.sleep(nanoseconds: 1_000_000_000)

		let messagesPerGroup = 50

		// Send messages to all groups simultaneously
		let sendTask = Task {
			await withTaskGroup(of: Void.self) { taskGroup in
				for (index, group) in groups.enumerated() {
					taskGroup.addTask {
						for i in 1...messagesPerGroup {
							_ = try? await group.send(content: "g\(index)-msg-\(i)")
						}
					}
				}
			}
		}

		// Add members to one group while messages are flowing to all
		let joinTask = Task {
			let joinKey = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
			let joinOpts = ClientOptions(api: apiOpts, dbEncryptionKey: joinKey)
			for _ in 1...3 {
				let k = try PrivateKey.generate()
				let c = try await Client.create(account: k, options: joinOpts)
				_ = try? await groups[0].addMembers(inboxIds: [c.inboxID])
			}
		}

		// Concurrent sync (like the real app)
		let syncTask = Task {
			for _ in 1...20 {
				_ = try? await boClient.conversations.syncAllConversations()
				try? await Task.sleep(nanoseconds: 50_000_000)
			}
		}

		_ = await sendTask.result
		_ = await joinTask.result
		_ = await syncTask.result
		try await Task.sleep(nanoseconds: 10_000_000_000)

		streamTask.cancel()

		let totalExpected = messagesPerGroup * groups.count
		let totalReceived = await receivedByGroup.totalCount()
		let perGroupCounts = await receivedByGroup.counts()

		print("=== streamAllMessages Across Multiple Groups ===")
		print("Groups: \(groups.count)")
		print("Messages per group: \(messagesPerGroup)")
		print("Total expected: \(totalExpected)")
		print("Total received via stream: \(totalReceived)")
		for (groupId, count) in perGroupCounts {
			print("  Group \(groupId.prefix(8))...: \(count)/\(messagesPerGroup)")
		}

		XCTAssertEqual(
			totalReceived, totalExpected,
			"Stream delivered \(totalReceived)/\(totalExpected) messages across \(groups.count) groups."
		)

		try alixClient.deleteLocalDatabase()
		try boClient.deleteLocalDatabase()
	}

	/// Test: aggressive syncAllConversations frequency during messaging.
	///
	/// The Convos app calls syncAllConversations on startup, on foreground resume,
	/// and during discovery polling (every 3 seconds). This test uses a much higher
	/// frequency to see if increased sync contention causes message loss even with
	/// non-blocking stream dispatch.
	func testAggressiveSyncFrequencyDoesNotLoseMessages() async throws {
		try TestConfig.skipIfNotRunningLocalNodeTests()

		let fixtures = try await fixtures()

		let alixGroup = try await fixtures.alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID]
		)
		try await fixtures.boClient.conversations.sync()
		let boGroup = try await fixtures.boClient.conversations.findGroup(
			groupId: alixGroup.id
		)!

		let receivedMessages = TestTranscript()
		let streamTask = Task(priority: .userInitiated) {
			for try await message in boGroup.streamMessages() {
				// Non-blocking dispatch
				let capturedMessage = message
				Task {
					if (try? capturedMessage.encodedContent.type == ContentTypeText) ?? false {
						let text = (try? capturedMessage.content() as String) ?? ""
						await receivedMessages.add(text)
					}
				}
			}
		}

		try await Task.sleep(nanoseconds: 1_000_000_000)

		let messageCount = 200

		let sendTask = Task {
			for i in 1...messageCount {
				_ = try? await alixGroup.send(content: "msg-\(i)")
			}
		}

		let joinTask = Task {
			let key = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
			let opts = ClientOptions(
				api: ClientOptions.Api(
					env: XMTPEnvironment.local,
					isSecure: XMTPEnvironment.local.isSecure
				),
				dbEncryptionKey: key
			)
			for _ in 1...4 {
				let newKey = try PrivateKey.generate()
				let newClient = try await Client.create(account: newKey, options: opts)
				_ = try? await alixGroup.addMembers(inboxIds: [newClient.inboxID])
			}
		}

		// Aggressive sync: every 5ms instead of every 3 seconds
		let syncTask = Task {
			for _ in 1...200 {
				_ = try? await fixtures.boClient.conversations.syncAllConversations()
				try? await Task.sleep(nanoseconds: 5_000_000) // 5ms
			}
		}

		// Also sync the specific group aggressively
		let groupSyncTask = Task {
			for _ in 1...200 {
				try? await boGroup.sync()
				try? await Task.sleep(nanoseconds: 5_000_000)
			}
		}

		_ = await sendTask.result
		_ = await joinTask.result
		_ = await syncTask.result
		_ = await groupSyncTask.result
		try await Task.sleep(nanoseconds: 10_000_000_000)

		streamTask.cancel()

		let streamCount = await receivedMessages.messages.count

		print("=== Aggressive Sync Frequency ===")
		print("Messages sent: \(messageCount)")
		print("Messages via stream: \(streamCount)")
		print("Sync calls: ~400 (syncAll + group sync at 5ms intervals)")

		XCTAssertEqual(
			streamCount, messageCount,
			"Stream delivered \(streamCount)/\(messageCount) messages under aggressive sync."
		)

		try fixtures.cleanUpDatabases()
	}

	/// Test: syncAllConversations called from multiple concurrent callers.
	///
	/// In the real app, multiple inboxes each call syncAllConversations independently.
	/// With 10+ inboxes, there could be significant database contention. This test
	/// simulates that by running multiple concurrent sync loops against the same
	/// XMTP network while messages are being delivered.
	func testConcurrentSyncCallersDoNotLoseMessages() async throws {
		try TestConfig.skipIfNotRunningLocalNodeTests()

		let apiOpts = ClientOptions.Api(
			env: XMTPEnvironment.local,
			isSecure: XMTPEnvironment.local.isSecure
		)

		let alixKey = try PrivateKey.generate()
		let alixDbKey = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
		let alixClient = try await Client.create(
			account: alixKey,
			options: ClientOptions(api: apiOpts, dbEncryptionKey: alixDbKey)
		)

		let boKey = try PrivateKey.generate()
		let boDbKey = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
		let boClient = try await Client.create(
			account: boKey,
			options: ClientOptions(api: apiOpts, dbEncryptionKey: boDbKey)
		)

		let alixGroup = try await alixClient.conversations.newGroup(
			with: [boClient.inboxID]
		)
		try await boClient.conversations.sync()
		let boGroup = try await boClient.conversations.findGroup(
			groupId: alixGroup.id
		)!

		// Create 8 additional independent inboxes that also sync
		var otherClients: [Client] = []
		for _ in 0..<8 {
			let key = try PrivateKey.generate()
			let dbKey = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
			let client = try await Client.create(
				account: key,
				options: ClientOptions(api: apiOpts, dbEncryptionKey: dbKey)
			)
			// Give each client a group to sync
			_ = try await alixClient.conversations.newGroup(with: [client.inboxID])
			otherClients.append(client)
		}

		let receivedMessages = TestTranscript()
		let streamTask = Task(priority: .userInitiated) {
			for try await message in boGroup.streamMessages() {
				let capturedMessage = message
				Task {
					if (try? capturedMessage.encodedContent.type == ContentTypeText) ?? false {
						let text = (try? capturedMessage.content() as String) ?? ""
						await receivedMessages.add(text)
					}
				}
			}
		}

		// Start streams on all other clients (simulates per-inbox SyncingManager)
		var otherStreamTasks: [Task<Void, Never>] = []
		for client in otherClients {
			let task = Task(priority: .userInitiated) {
				do {
					for try await _ in await client.conversations.streamAllMessages() {}
				} catch {}
			}
			otherStreamTasks.append(task)
		}

		try await Task.sleep(nanoseconds: 1_000_000_000)

		let messageCount = 200

		let sendTask = Task {
			for i in 1...messageCount {
				_ = try? await alixGroup.send(content: "msg-\(i)")
			}
		}

		let joinTask = Task {
			let joinKey = Data((0..<32).map { _ in UInt8.random(in: 0...255) })
			let joinOpts = ClientOptions(api: apiOpts, dbEncryptionKey: joinKey)
			for _ in 1...4 {
				let k = try PrivateKey.generate()
				let c = try await Client.create(account: k, options: joinOpts)
				_ = try? await alixGroup.addMembers(inboxIds: [c.inboxID])
			}
		}

		// All clients sync simultaneously
		let syncTask = Task {
			for _ in 1...20 {
				await withTaskGroup(of: Void.self) { group in
					group.addTask {
						_ = try? await boClient.conversations.syncAllConversations()
					}
					for client in otherClients {
						group.addTask {
							_ = try? await client.conversations.syncAllConversations()
						}
					}
				}
				try? await Task.sleep(nanoseconds: 50_000_000)
			}
		}

		_ = await sendTask.result
		_ = await joinTask.result
		_ = await syncTask.result
		try await Task.sleep(nanoseconds: 10_000_000_000)

		streamTask.cancel()
		for task in otherStreamTasks { task.cancel() }

		let streamCount = await receivedMessages.messages.count

		print("=== Concurrent Sync Callers (\(otherClients.count + 1) inboxes) ===")
		print("Messages sent: \(messageCount)")
		print("Messages via stream: \(streamCount)")

		XCTAssertEqual(
			streamCount, messageCount,
			"Stream delivered \(streamCount)/\(messageCount) with \(otherClients.count + 1) concurrent syncing inboxes."
		)

		try alixClient.deleteLocalDatabase()
		try boClient.deleteLocalDatabase()
		for c in otherClients { try c.deleteLocalDatabase() }
	}
}

// MARK: - Test helpers

/// Thread-safe message collector for tracking received messages per group.
actor TestGroupTranscript {
	private var messages: [String: [String]] = [:]
	private let groupCount: Int

	init(groupCount: Int) {
		self.groupCount = groupCount
	}

	func add(_ text: String, groupId: String) {
		if messages[groupId] == nil {
			messages[groupId] = []
		}
		messages[groupId]?.append(text)
	}

	func totalCount() -> Int {
		messages.values.reduce(0) { $0 + $1.count }
	}

	func counts() -> [(String, Int)] {
		messages.map { ($0.key, $0.value.count) }.sorted { $0.1 > $1.1 }
	}
}
