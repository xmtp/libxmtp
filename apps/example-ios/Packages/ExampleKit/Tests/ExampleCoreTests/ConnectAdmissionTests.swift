@testable import ExampleCore
import Foundation
import Testing

@MainActor private final class AdmissionSecrets: SecretStore {
	var values: [UUID: [String: Data]] = [:]
	func read(_ id: UUID, ref: String) -> Data? {
		values[id]?[ref]
	}

	func write(_ id: UUID, ref: String, data: Data) {
		values[id, default: [:]][ref] = data
	}

	func remove(_ id: UUID, ref: String) {
		values[id]?[ref] = nil
	}

	func removeAll(_ id: UUID) {
		values[id] = nil
	}

	func randomBytes(count: Int) -> Data {
		Data(repeating: 1, count: count)
	}
}

@MainActor private final class AdmissionFiles: ProfileFileStore {
	var index = SessionIndex()
	var signedURLs: [String] = []
	func loadIndex() -> SessionIndex {
		index
	}

	func saveIndex(_ value: SessionIndex) {
		index = value
		if value.signedIn,
		   let profile = value.profiles.first(where: { $0.id == value.activeID })
		{
			signedURLs.append(profile.url)
		}
	}

	func paths(_ id: UUID) -> ProfilePaths {
		ProfilePaths(root: URL(fileURLWithPath: "/tmp/" + id.uuidString))
	}

	func databaseExists(_: UUID) -> Bool {
		false
	}

	func removeDatabase(_: UUID) {}
	func removeExports(_: UUID) {}
	func removeProfile(_: UUID) {}
}

@MainActor private final class AdmissionClient: SessionClient {
	let inboxID: String
	let identity = "identity"
	var ended = false
	init(_ inboxID: String) {
		self.inboxID = inboxID
	}

	func end() async {
		ended = true
	}

	func disableNotifications() async {}
}

@MainActor private final class AdmissionFactory: SessionFactory {
	var holdFirst = true
	var pending: CheckedContinuation<Void, Never>?
	var clients: [AdmissionClient] = []
	// Mirrors SessionFactory security and storage inputs.
	// swiftlint:disable:next function_parameter_count
	func open(profile _: BackendProfile, paths _: ProfilePaths, wallet _: Data, databaseKey _: Data,
	          secrets _: any SecretStore, key _: SessionKey, fence _: SessionFence) async throws -> any SessionClient
	{
		let client = AdmissionClient(clients.isEmpty ? "old-inbox" : "new-inbox"); clients.append(client)
		if clients.count == 1, holdFirst {
			await withCheckedContinuation { pending = $0 }
		}
		return client
	}
}

private struct AdmissionProbe: BackendProbe {
	func probe(_: BackendURL) async throws -> BackendProbeResult {
		BackendProbeResult(requiresToken: false)
	}
}

@MainActor struct ConnectAdmissionTests {
	@Test(arguments: [false, true])
	func changedServerCannotPublishHeldOldOpen(joinNewBeforeOldReturns: Bool) async throws {
		let files = AdmissionFiles(); let factory = AdmissionFactory()
		let session = SessionController(fence: SessionFence(), secrets: AdmissionSecrets(), files: files, factory: factory)
		let store = ConnectStore(
			probe: AdmissionProbe(),
			session: session,
			owner: RouteOwner(),
			defaultURL: "https://old.example",
		)
		store.send(.appear); try await ready { store.state.ready }
		store.send(.join); try await ready { factory.pending != nil }
		store.send(.addressChanged("https://new.example")); try await ready { store.state.ready }
		if joinNewBeforeOldReturns {
			store.send(.join); try await ready { session.client?.inboxID == "new-inbox" }
		}
		factory.pending?.resume(); factory.pending = nil
		if !joinNewBeforeOldReturns {
			try await ready { session.state != .opening }
			#expect(session.client == nil); #expect(session.state == .signedOut)
			#expect(!files.index.signedIn)
		} else {
			try await ready { factory.clients.first?.ended == true }
			#expect(session.client?.inboxID == "new-inbox")
		}
		#expect(factory.clients.first?.ended == true)
		#expect(!files.signedURLs.contains("https://old.example"))
		#expect(store.state.address == "https://new.example"); #expect(store.state.error == nil)
		if !joinNewBeforeOldReturns {
			store.send(.join); try await ready { session.client?.inboxID == "new-inbox" }
		}
		#expect(files.signedURLs == ["https://new.example"])
		#expect(store.state.ready); #expect(!store.state.joining)
	}

	@Test func canceledQueuedConnectNeverCallsSDKFactory() async {
		let files = AdmissionFiles(); let factory = AdmissionFactory(); factory.holdFirst = false
		let session = SessionController(fence: SessionFence(), secrets: AdmissionSecrets(), files: files, factory: factory)
		let pending = Task { try await session.connect(url: "https://old.example", token: "") }; pending.cancel()
		var cancelled = false
		do { try await pending.value } catch is CancellationError { cancelled = true } catch { Issue.record(error) }
		#expect(cancelled); #expect(factory.clients.isEmpty)
		#expect(session.client == nil); #expect(files.signedURLs.isEmpty)
	}

	private func ready(_ condition: () -> Bool) async throws {
		let clock = ContinuousClock(); let deadline = clock.now.advanced(by: .seconds(3))
		while !condition(), clock.now < deadline {
			await Task.yield()
		}
		try #require(condition())
	}
}
