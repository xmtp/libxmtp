@testable import ExampleCore
import Foundation
import Testing

@MainActor final class PushFixture {
	let owner = PushOwner(
		session: SessionKey(profileID: UUID(), generation: 1),
		installation: String(repeating: "01", count: 32),
	)
	let gateway = PushFakeGateway(); let preferences = PushFakePreferences(); let publisher = PushFakePublisher()
	var current = true; var permission = true
	var didCommit: @MainActor () async -> Void = {}
	lazy var controller = PushController(
		configured: true,
		owner: owner,
		gateway: gateway,
		preferences: preferences,
		publisher: publisher,
		current: { [unowned self] in current ? owner : nil },
		permitted: { [unowned self] in permission },
		signedIn: { true },
		didCommit: { [unowned self] in await didCommit() },
	)
}

@MainActor final class PushFakeGateway: NotificationGateway {
	var installation = String(repeating: "01", count: 32)
	var registrationState: PushRegistrationState = .disabled
	var enableResult: PushRegistrationState = .enabled
	var stateReads = 0
	var admitted = true; var onRead: (() -> Void)?; var calls: [String] = []; var holdEnable = false
	var notificationWrites = 0; var beforeNotificationWrite: (() -> Void)?
	var enableContinuation: CheckedContinuation<PushRegistrationState, Never>?
	func state() -> PushRegistrationState {
		stateReads += 1; return registrationState
	}

	func enable(token _: String) async -> PushRegistrationState {
		calls
			.append("enable"); if holdEnable
		{
			return await withCheckedContinuation { enableContinuation = $0 }
		}; return enableResult
	}

	func disable() async {
		calls.append("disable")
	}

	func conversation(id _: String) async -> PushConversation? {
		await Task.yield(); onRead?(); return PushConversation(
			id: "chat",
			logicalKey: "logical",
			admitted: admitted,
		)
	}

	func setEnabled(_: Bool, conversationID _: String) async {
		beforeNotificationWrite?(); notificationWrites += 1
	}
}

@MainActor final class PushFakePreferences: PushPreferences, ConversationPreferences {
	var enabled = true; var muted = false; var posted: Set<String> = []
	var failNotificationWrite = false
	var dismissedRequests: Set<String> {
		[]
	}

	func alias(for _: String) -> String? {
		nil
	}

	func marker(for _: String) -> Int64 {
		0
	}

	func dismissRequest(_: String) {}
	var notificationsEnabled: Bool {
		enabled
	}

	func setNotificationsEnabled(_ value: Bool) throws {
		if failNotificationWrite {
			throw NotificationPreferenceWriteError.failed
		}
		enabled = value
	}

	func muted(for _: String) -> Bool {
		muted
	}

	func setMute(_ value: Bool, for _: String) {
		muted = value
	}

	func hasPosted(_ tag: String) -> Bool {
		posted.contains(tag)
	}

	func recordPosted(_ tag: String) {
		posted.insert(tag)
	}
}

@MainActor final class PushFakePublisher: PushPublisher {
	var posts = 0; var held = false; var removed: Set<String> = []
	var completion: (@MainActor (Bool) -> Void)?
	func post(tag _: String, route _: PushRoute, completion: @escaping @MainActor (Bool) -> Void) {
		posts += 1; if held {
			self.completion = completion
		} else {
			completion(true)
		}
	}

	func complete() {
		let callback = completion; completion = nil; callback?(true)
	}

	func remove(tag: String) {
		removed.insert(tag)
	}

	func remove(conversationID _: String) {}

	func removeAll() {}
}

@MainActor final class PushSessionFixture {
	let profile = BackendProfile(url: "https://example.com", inboxID: "inbox", identity: "identity")
	let files = PushFiles(); let secrets = PushSecrets(); let factory = PushFactory()
	lazy var session = SessionController(fence: SessionFence(), secrets: secrets, files: files, factory: factory)
	init() {
		files.index.profiles = [profile]; files.index.activeID = profile.id; files.index.signedIn = true
	}
}

@MainActor final class PushFiles: ProfileFileStore {
	var index = SessionIndex(); var hasDatabase = true
	func loadIndex() -> SessionIndex {
		index
	}

	func saveIndex(_ value: SessionIndex) {
		index = value
	}

	func paths(_ profile: UUID) -> ProfilePaths {
		ProfilePaths(root: URL(fileURLWithPath: "/tmp/" + profile.uuidString))
	}

	func databaseExists(_: UUID) -> Bool {
		hasDatabase
	}

	func removeDatabase(_: UUID) {}
	func removeExports(_: UUID) {}
	func removeProfile(_: UUID) {}
}

@MainActor final class PushSecrets: SecretStore {
	var values = [
		"wallet": Data(repeating: 1, count: 32),
		"database-key": Data(repeating: 1, count: 32),
		"preferences-key": Data(repeating: 1, count: 32),
	]
	var writes = 0
	func read(_: UUID, ref: String) -> Data? {
		values[ref]
	}

	func write(_: UUID, ref: String, data: Data) {
		writes += 1; values[ref] = data
	}

	func remove(_: UUID, ref _: String) {}
	func removeAll(_: UUID) {}
	func randomBytes(count: Int) -> Data {
		Data(repeating: 0, count: count)
	}
}

@MainActor final class PushFactory: SessionFactory {
	var failure: SessionError?
	var opens = 0; var hold = false; var waiter: CheckedContinuation<Void, Never>?
	let client = PushClient()
	// Matches the session factory protocol; each security input stays explicit.
	// swiftlint:disable:next function_parameter_count
	func open(
		profile _: BackendProfile,
		paths _: ProfilePaths,
		wallet _: Data,
		databaseKey _: Data,
		secrets _: any SecretStore,
		key _: SessionKey,
		fence _: SessionFence,
	) async throws -> any SessionClient {
		opens += 1
		if hold {
			await withCheckedContinuation { waiter = $0 }
		}
		if let failure {
			throw failure
		}
		return client
	}
}

@MainActor final class PushClient: SessionClient {
	let inboxID = "inbox"; let identity = "identity"; var ended = false
	func end() async {
		ended = true
	}

	func disableNotifications() async {}
}

struct PushRowQueries: ConversationQueries {
	func list(consent _: ConversationConsent, limit _: Int) async throws -> [ConversationRecord] {
		[ConversationRecord(
			id: "chat",
			kind: .group,
		)]
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		record
	}

	func setConsent(id _: String, consent _: ConversationConsent) async throws {}
}

private enum NotificationPreferenceWriteError: Error { case failed }
