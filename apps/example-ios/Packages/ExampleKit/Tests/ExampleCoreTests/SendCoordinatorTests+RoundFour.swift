import ExampleCore
import Foundation
import Testing

extension SendCoordinatorTests {
	@Test func failedDiscardAllowsExplicitRetryWithoutRevivingOldSend() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(); let gate = ReviewGate()
		fixture.gateway.uploadHook = { await gate.wait() }
		fixture.gateway.discardHook = { throw ChatError.unavailable }
		fixture.store.send(.send(draft.id))
		for _ in 0 ..< 10000 {
			if gate.started {
				break
			}; await Task.yield()
		}
		try #require(gate.started)
		fixture.store.send(.discard(draft.id))
		for _ in 0 ..< 10000 {
			if fixture.store.state.error != nil {
				break
			}; await Task.yield()
		}
		try #require(fixture.store.state.error != nil)
		gate.release(); await fixture.store.wait()
		#expect(fixture.queries.queues == 0)
		#expect(try fixture.drafts.all().count == 1)
		fixture.gateway.uploadHook = nil
		fixture.store.send(.send(draft.id)); await fixture.store.wait()
		#expect(fixture.queries.queues == 1)
	}

	@Test func lateImportAndClosedStoreRemoveOnlyOwnedStagingFiles() async throws {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		defer { do { try FileManager.default.removeItem(at: root) } catch { Issue.record("Cleanup failed: \(error)") } }
		let staging = AttachmentStagingFiles(temporaryDirectory: root)
		let late = try staging.newFile(); try Data([1]).write(to: late)
		let imports = AttachmentImportFiles(owner: staging)
		try await imports.close()
		var rejected = false
		do { try await imports.record(late) } catch { rejected = true }
		#expect(rejected); #expect(!FileManager.default.fileExists(atPath: late.path))
		let staged = try staging.newFile(); try Data([2]).write(to: staged)
		let original = root.appendingPathComponent("original.jpg"); try Data([3]).write(to: original)
		let fixture = AttachmentFixture(); var creates = 0
		fixture.gateway.createHook = { _, _ in creates += 1; return fixture.file }
		let origin = DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1)
		let store = AttachmentStore(gateway: fixture.gateway, staging: staging, drafts: fixture.drafts,
		                            coordinator: fixture.coordinator, conversationID: "chat",
		                            origin: origin,
		                            admit: { _ in true }, allowed: { true })
		await store.close()
		store.send(.stage(staged, "photo.jpg")); store.send(.stage(original, "original.jpg"))
		await store.wait()
		#expect(creates == 0); #expect(!FileManager.default.fileExists(atPath: staged.path))
		#expect(try Data(contentsOf: original) == Data([3]))
	}
}

extension MessageMapperTests {
	@MainActor @Test func overlapRefreshRetainsNewestPrefixStatus() {
		let cache = TranscriptCache()
		let rows = (1 ... 6).reversed().map { value in ChatMessage(id: String(value), conversationID: "chat", sender: "peer",
		                                                           sentAt: Int64(value), content: .text("text")) }
		var original = HistoryWindow(); original.rows = rows; original.atNewest = true
		_ = cache.put("chat", window: original)
		var page = HistoryWindow(); page.rows = Array(rows.dropFirst(2)); page.atNewest = false
		let refreshed = cache.refresh("chat", page: page, anchor: nil)
		#expect(refreshed.rows.map(\.id) == rows.map(\.id)); #expect(refreshed.atNewest)
	}
}

extension SendCoordinatorTests {
	@Test func failedImportCleanupRetainsFilesForRetry() async throws {
		let manager = FileManager.default; let root = manager.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		defer { do { try manager.removeItem(at: root) } catch { Issue.record("Cleanup failed: \(error)") } }
		let owner = AttachmentStagingFiles(temporaryDirectory: root); let first = try owner.newFile(); let second = try owner
			.newFile()
		try Data([1]).write(to: first); try Data([2]).write(to: second)
		let imports = AttachmentImportFiles(owner: owner); try await imports.record(first); try await imports.record(second)
		let backup = root.appendingPathComponent("backup"); try manager
			.moveItem(at: owner.directory, to: backup); try Data([0]).write(to: owner.directory)
		var failed = false; do { try await imports.close() } catch { failed = true }; #expect(failed)
		try manager.removeItem(at: owner.directory); try manager.moveItem(at: backup, to: owner.directory)
		try await imports
			.close(); #expect(!manager.fileExists(atPath: first.path)); #expect(!manager.fileExists(atPath: second.path))
	}
}
