import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing
@testable import XmtpSdk

struct ChatConsentEnrichmentTests {
	@Test(arguments: [ConsentState.allowed, .denied])
	func offlineHandleLookupPreservesLocalConsent(consent: ConsentState) async throws {
		let probe = ChatIdentityProbe()
		let client = SDKClient(OfflineChatClient(consent: consent, probe: probe), codecs: [])
		let queries = SDKChatQueries(client: client, limiter: AsyncLimiter())
		let record = try await queries.conversation(id: "dm")
		#expect(record.consent == (consent == .allowed ? .allowed : .denied))
		#expect(record.peerInboxID == "peer")
		#expect(record.peerHandle == nil)
		#expect(await probe.refreshes == [false])
		try await client.end()
	}

	@Test func canceledHandleLookupStillCancelsConversationRead() async throws {
		let probe = ChatIdentityProbe(cancel: true)
		let client = SDKClient(OfflineChatClient(consent: .allowed, probe: probe), codecs: [])
		let queries = SDKChatQueries(client: client, limiter: AsyncLimiter())
		await #expect(throws: CancellationError.self) { try await queries.conversation(id: "dm") }
		#expect(await probe.refreshes == [false])
		try await client.end()
	}
}

private enum ChatIdentityError: Error { case offline }
private actor ChatIdentityProbe {
	let cancel: Bool
	var refreshes: [Bool] = []
	init(cancel: Bool = false) {
		self.cancel = cancel
	}

	func lookup(refresh: Bool) throws -> [InboxState] {
		refreshes.append(refresh)
		if cancel {
			throw CancellationError()
		}
		throw ChatIdentityError.offline
	}
}

private final class OfflineChatClient: Client, @unchecked Sendable {
	let values: OfflineChatConversations
	let probe: ChatIdentityProbe
	init(consent: ConsentState, probe: ChatIdentityProbe) {
		values = OfflineChatConversations(consent: consent); self.probe = probe
		super.init(noHandle: NoHandle())
	}

	required init(unsafeFromHandle _: UInt64) {
		fatalError("Fake has no native handle")
	}

	override func clientKey() -> UInt64 {
		UInt64(truncatingIfNeeded: ObjectIdentifier(self).hashValue)
	}

	override func conversations() -> Conversations {
		values
	}

	override func inboxStates(ids _: [InboxId], refreshFromNetwork: Bool) async throws -> [InboxState] {
		try await probe.lookup(refresh: refreshFromNetwork)
	}

	override func end() async throws {}
}

private final class OfflineChatConversations: Conversations, @unchecked Sendable {
	let value: OfflineChatDm
	init(consent: ConsentState) {
		value = OfflineChatDm(consent: consent); super.init(noHandle: NoHandle())
	}

	required init(unsafeFromHandle _: UInt64) {
		fatalError("Fake has no native handle")
	}

	override func getById(id _: ConversationId) async throws -> Conversation? {
		.dm(dm: value)
	}
}

private final class OfflineChatDm: Dm, @unchecked Sendable {
	let consent: ConsentState
	init(consent: ConsentState) {
		self.consent = consent; super.init(noHandle: NoHandle())
	}

	required init(unsafeFromHandle _: UInt64) {
		fatalError("Fake has no native handle")
	}

	override func peerInboxId() async throws -> InboxId? {
		"peer"
	}

	override func state() async throws -> ConversationState {
		ConversationState(isActive: true, consentState: consent, pausedForVersion: nil,
		                  isDisappearingEnabled: false, disappearingSettings: nil, notificationsEnabled: true,
		                  commitLogForkStatus: .notForked)
	}
}
