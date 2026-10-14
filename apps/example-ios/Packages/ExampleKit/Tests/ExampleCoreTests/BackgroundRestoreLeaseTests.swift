@testable import ExampleCore
import Foundation
import Testing

@MainActor struct BackgroundRestoreLeaseTests {
	@Test(arguments: [false, true])
	func expiredHeldOpenClosesClientBeforePublication(cancelWork: Bool) async throws {
		let fixture = PushSessionFixture(); fixture.factory.hold = true
		let lease = PushLease(); var callbacks = 0
		let completion = PushCompletion<Int>(lease: lease) { _ in callbacks += 1 }
		let pending = Task { await fixture.session.restoreForPush(lease: lease) }
		try await held(fixture)
		#expect(fixture.session.client == nil)
		#expect(completion.finish(0)); #expect(callbacks == 1); #expect(!lease.active)
		if cancelWork {
			pending.cancel()
		}
		fixture.factory.waiter?.resume(); fixture.factory.waiter = nil
		#expect(await pending.value == nil)
		#expect(fixture.factory.client.ended); #expect(fixture.session.client == nil)
		#expect(fixture.session.state == .signedOut); #expect(!fixture.session.backgroundOnly)
		#expect(fixture.files.index.signedIn); #expect(fixture.secrets.writes == 0)
		fixture.factory.hold = false
		#expect(await fixture.session.restoreForPush() != nil)
		#expect(fixture.factory.opens == 2)
	}

	@Test func foregroundAdoptsHeldOpenAfterFetchExpires() async throws {
		let fixture = PushSessionFixture(); fixture.factory.hold = true
		let lease = PushLease()
		let pending = Task { await fixture.session.restoreForPush(lease: lease) }
		try await held(fixture)
		fixture.factory.hold = false
		await fixture.session.restore()
		lease.cancel(); pending.cancel()
		fixture.factory.waiter?.resume(); fixture.factory.waiter = nil
		#expect(await pending.value == nil)
		#expect(fixture.session.client != nil); #expect(!fixture.factory.client.ended)
		#expect(!fixture.session.backgroundOnly)
		#expect(fixture.factory.opens == 1)
		guard case let .active(info) = fixture.session.state else { Issue.record("Foreground did not adopt session"); return }
		#expect(fixture.session.fence.accepts(info.key))
	}

	@Test func expiredFetchDoesNotClosePreviouslyActiveSession() async {
		let fixture = PushSessionFixture()
		let initial = await fixture.session.restoreForPush()
		#expect(initial != nil)
		let state = fixture.session.state
		let lease = PushLease(); lease.cancel()
		let pending = Task { await fixture.session.restoreForPush(lease: lease) }; pending.cancel()
		#expect(await pending.value == nil)
		#expect(fixture.session.state == state); #expect(fixture.session.client != nil)
		#expect(!fixture.factory.client.ended); #expect(fixture.factory.opens == 1)
	}

	@Test func cancelledForegroundOpenDoesNotPublishReturnedClient() async throws {
		let fixture = PushSessionFixture(); fixture.factory.hold = true
		let pending = Task { await fixture.session.restore() }
		try await held(fixture); pending.cancel()
		fixture.factory.waiter?.resume(); fixture.factory.waiter = nil
		await pending.value
		#expect(fixture.factory.client.ended); #expect(fixture.session.client == nil)
		#expect(fixture.session.state == .restoreFailed)
	}

	@Test func foregroundDuringAbandonedCleanupStartsFreshRestore() async throws {
		let fixture = PushSessionFixture(); let factory = ReopeningPushFactory()
		let session = SessionController(
			fence: SessionFence(),
			secrets: fixture.secrets,
			files: fixture.files,
			factory: factory,
		)
		var cleanup: CheckedContinuation<Void, Never>?
		session.beforeEnd = { await withCheckedContinuation { cleanup = $0 } }
		let lease = PushLease()
		let pending = Task { await session.restoreForPush(lease: lease) }
		try await ready { factory.waiter != nil }
		lease.cancel(); pending.cancel(); factory.waiter?.resume(); factory.waiter = nil
		try await ready { cleanup != nil }
		await session.restore()
		cleanup?.resume(); cleanup = nil
		#expect(await pending.value == nil)
		try await ready { session.client != nil }
		#expect(factory.opens == 2)
		#expect(factory.clients.first?.ended == true)
		#expect(factory.clients.last?.ended == false)
		#expect(session.client === factory.clients.last)
		#expect(!session.backgroundOnly)
		guard case let .active(info) = session.state else { Issue.record("Fresh foreground restore not active"); return }
		#expect(session.fence.accepts(info.key))
	}

	private func ready(_ condition: () -> Bool) async throws {
		let clock = ContinuousClock(); let deadline = clock.now.advanced(by: .seconds(2))
		while !condition(), clock.now < deadline {
			await Task.yield()
		}
		try #require(condition())
	}

	private func held(_ fixture: PushSessionFixture) async throws {
		let clock = ContinuousClock(); let deadline = clock.now.advanced(by: .seconds(2))
		while fixture.factory.waiter == nil, clock.now < deadline {
			await Task.yield()
		}
		try #require(fixture.factory.waiter != nil)
	}
}

@MainActor private final class ReopeningPushFactory: SessionFactory {
	var opens = 0
	var waiter: CheckedContinuation<Void, Never>?
	var clients: [PushClient] = []
	// Mirrors the explicit identity and storage inputs of SessionFactory.
	// swiftlint:disable:next function_parameter_count
	func open(profile _: BackendProfile, paths _: ProfilePaths, wallet _: Data, databaseKey _: Data,
	          secrets _: any SecretStore, key _: SessionKey, fence _: SessionFence) async throws -> any SessionClient
	{
		opens += 1
		let client = PushClient(); clients.append(client)
		if opens == 1 {
			await withCheckedContinuation { waiter = $0 }
		}
		return client
	}
}
