import ExampleCore
import Foundation
import Testing

@MainActor struct AttachmentDownloadCancellationTests {
	@Test func invalidationCancelsHeldDownloadAndAllowsFreshRequest() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add()
		let held = CancellableDownloadGate()
		fixture.gateway.downloadHook = { try await held.wait() }
		fixture.store.send(.download("message", draft.descriptor))
		await settle { held.started }
		try #require(held.started)
		fixture.store.invalidateDownloads()
		await settle { held.cancelled }
		#expect(held.cancelled)
		held.release()
		await fixture.store.wait()
		#expect(fixture.store.verified.get("message") == nil)
		#expect(fixture.store.state.cards["message"]?.phase == .available)
		#expect(fixture.store.state.error == nil)
		fixture.gateway.downloadHook = nil
		fixture.store.send(.download("message", draft.descriptor)); await fixture.store.wait()
		#expect(fixture.gateway.downloads == 2)
		#expect(fixture.store.state.cards["message"]?.phase == .verified)
		await fixture.store.close()
	}

	@Test func invalidationCancelsAllDownloadsButKeepsUploadAlive() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add()
		let first = CancellableDownloadGate(); let second = CancellableDownloadGate()
		let upload = CancellableDownloadGate()
		fixture.gateway.downloadHook = {
			if !first.started {
				try await first.wait()
			} else {
				try await second.wait()
			}
		}
		fixture.gateway.uploadHook = { try await upload.wait() }
		fixture.store.send(.send(draft.id))
		fixture.store.send(.download("one", draft.descriptor))
		fixture.store.send(.download("two", draft.descriptor))
		await settle { first.started && second.started && upload.started }
		try #require(first.started && second.started && upload.started)
		fixture.store.invalidateDownloads()
		await settle { first.cancelled && second.cancelled }
		#expect(first.cancelled && second.cancelled)
		#expect(!upload.cancelled)
		first.release(); second.release(); upload.release(); await fixture.store.wait()
		#expect(fixture.queries.queues == 1)
		#expect(fixture.queries.publications == 1)
		await fixture.store.close()
	}

	@Test func invalidationLeavesAnotherStoreConsumerActive() async throws {
		let revoked = AttachmentFixture(); let active = AttachmentFixture()
		let draft = try revoked.add(); let revokedGate = CancellableDownloadGate()
		let activeGate = CancellableDownloadGate()
		revoked.gateway.downloadHook = { try await revokedGate.wait() }
		active.gateway.downloadHook = { try await activeGate.wait() }
		revoked.store.send(.download("message", draft.descriptor))
		active.store.send(.download("message", draft.descriptor))
		await settle { revokedGate.started && activeGate.started }
		try #require(revokedGate.started && activeGate.started)
		revoked.store.invalidateDownloads()
		await settle { revokedGate.cancelled }
		#expect(revokedGate.cancelled)
		#expect(!activeGate.cancelled)
		#expect(active.store.state.cards["message"]?.phase == .downloading)
		revokedGate.release(); activeGate.release()
		await revoked.store.wait(); await active.store.wait()
		#expect(revoked.store.verified.get("message") == nil)
		#expect(active.store.verified.get("message") != nil)
		await revoked.store.close(); await active.store.close()
	}

	@Test func invalidationBeforeTaskStartsPreventsRequest() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add()
		fixture.store.send(.download("message", draft.descriptor))
		fixture.store.invalidateDownloads()
		await fixture.store.wait()
		#expect(fixture.gateway.downloads == 0)
		#expect(fixture.store.verified.get("message") == nil)
		await fixture.store.close()
	}

	private func settle(_ completed: () -> Bool) async {
		for _ in 0 ..< 10000 {
			if completed() {
				return
			}
			await Task.yield()
		}
	}
}

@MainActor private final class CancellableDownloadGate {
	var started = false
	var cancelled = false
	private var continuation: CheckedContinuation<Void, any Error>?
	func wait() async throws {
		started = true
		try await withTaskCancellationHandler {
			try Task.checkCancellation()
			try await withCheckedThrowingContinuation { continuation = $0 }
		} onCancel: {
			Task { @MainActor in
				self.cancelled = true
				self.continuation?.resume(throwing: CancellationError())
				self.continuation = nil
			}
		}
	}

	func release() {
		continuation?.resume(); continuation = nil
	}
}
