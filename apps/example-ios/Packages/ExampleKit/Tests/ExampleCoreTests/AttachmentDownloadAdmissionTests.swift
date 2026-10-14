import ExampleCore
import Foundation
import Testing

@MainActor struct AttachmentDownloadAdmissionTests {
	@Test func rapidTapsDuringAuthorizationInvokeGatewayOnce() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(); let authorization = DownloadAuthorizationGate()
		let store = makeStore(fixture, authorization: authorization)
		for _ in 0 ..< 8 {
			store.send(.download("message", draft.descriptor))
		}
		await settle { authorization.calls > 0 }
		#expect(authorization.calls == 1)
		#expect(fixture.gateway.downloads == 0)
		authorization.releaseAll(true); await store.wait()
		#expect(fixture.gateway.downloads == 1)
		#expect(store.state.cards["message"]?.phase == .verified)
		await store.close()
	}

	@Test func deniedAuthorizationReleasesAdmissionForRetry() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(); let authorization = DownloadAuthorizationGate()
		let store = makeStore(fixture, authorization: authorization)
		store.send(.download("message", draft.descriptor)); await settle { authorization.calls == 1 }
		authorization.releaseAll(false); await store.wait()
		#expect(fixture.gateway.downloads == 0)
		store.send(.download("message", draft.descriptor)); await settle { authorization.calls == 2 }
		#expect(authorization.calls == 2)
		authorization.releaseAll(true); await store.wait()
		#expect(fixture.gateway.downloads == 1)
		await store.close()
	}

	@Test func oldAuthorizationCannotReleaseReplacementAdmission() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(); let authorization = DownloadAuthorizationGate()
		let store = makeStore(fixture, authorization: authorization)
		store.send(.download("message", draft.descriptor)); await settle { authorization.calls == 1 }
		store.invalidateDownloads()
		store.send(.download("message", draft.descriptor)); await settle { authorization.calls == 2 }
		authorization.releaseFirst(true)
		for _ in 0 ..< 100 {
			await Task.yield()
		}
		store.send(.download("message", draft.descriptor))
		for _ in 0 ..< 100 {
			await Task.yield()
		}
		#expect(authorization.calls == 2)
		#expect(fixture.gateway.downloads == 0)
		authorization.releaseAll(true); await store.wait()
		#expect(fixture.gateway.downloads == 1)
		#expect(store.state.cards["message"]?.phase == .verified)
		await store.close()
	}

	@Test func revokedConsentDuringAuthorizationPreventsDownload() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(); let authorization = DownloadAuthorizationGate()
		var allowed = true
		let store = makeStore(fixture, authorization: authorization, allowed: { allowed })
		store.send(.download("message", draft.descriptor)); await settle { authorization.calls == 1 }
		allowed = false; authorization.releaseAll(true); await store.wait()
		#expect(fixture.gateway.downloads == 0)
		#expect(store.verified.get("message") == nil)
		await store.close()
	}

	private func makeStore(_ fixture: AttachmentFixture, authorization: DownloadAuthorizationGate,
	                       allowed: @escaping () -> Bool = { true }) -> AttachmentStore
	{
		AttachmentStore(gateway: fixture.gateway, drafts: fixture.drafts, coordinator: fixture.coordinator,
		                conversationID: "chat", origin: DestinationOrigin(
		                	session: SessionKey(profileID: UUID(), generation: 1), token: 1,
		                ),
		                admit: { _ in true }, allowed: allowed, authorizeDownload: { await authorization.wait() })
	}

	private func settle(_ complete: () -> Bool) async {
		for _ in 0 ..< 10000 {
			if complete() {
				return
			}
			await Task.yield()
		}
	}
}

@MainActor private final class DownloadAuthorizationGate {
	var calls = 0
	private var pending: [CheckedContinuation<Bool, Never>] = []
	func wait() async -> Bool {
		calls += 1
		return await withCheckedContinuation { pending.append($0) }
	}

	func releaseFirst(_ allowed: Bool) {
		if !pending.isEmpty {
			pending.removeFirst().resume(returning: allowed)
		}
	}

	func releaseAll(_ allowed: Bool) {
		let waiting = pending; pending.removeAll()
		waiting.forEach { $0.resume(returning: allowed) }
	}
}
