import ExampleCore
import Foundation
import Testing

extension SendCoordinatorTests {
	@Test func invalidatedVerifiedFileCanDownloadAgain() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add()
		fixture.store.send(.download("message", draft.descriptor)); await fixture.store.wait()
		#expect(fixture.store.state.cards["message"]?.phase == .verified)
		fixture.store.invalidateDownloads()
		#expect(fixture.store.verified.get("message") == nil)
		#expect(fixture.store.state.cards["message"]?.phase == .available)
		fixture.store.send(.download("message", draft.descriptor)); await fixture.store.wait()
		#expect(fixture.gateway.downloads == 2)
		#expect(fixture.store.verified.get("message") != nil)
		#expect(fixture.store.state.cards["message"]?.phase == .verified)
		await fixture.store.close()
	}

	@Test(arguments: [false, true]) func invalidatedDownloadCannotRestoreCache(fails: Bool) async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(); let gate = ReviewGate()
		fixture.gateway.downloadHook = {
			await gate.wait()
			if fails {
				throw ChatError.unavailable
			}
		}
		fixture.store.send(.download("message", draft.descriptor))
		for _ in 0 ..< 10000 {
			if gate.started {
				break
			}; await Task.yield()
		}
		try #require(gate.started)
		fixture.store.invalidateDownloads(); gate.release(); await fixture.store.wait()
		#expect(fixture.store.verified.get("message") == nil)
		#expect(fixture.store.state.cards["message"]?.phase == .available)
		#expect(fixture.store.state.cards["message"]?.error == nil)
		fixture.gateway.downloadHook = nil
		fixture.store.send(.download("message", draft.descriptor)); await fixture.store.wait()
		#expect(fixture.store.verified.get("message") != nil)
		await fixture.store.close()
	}
}
