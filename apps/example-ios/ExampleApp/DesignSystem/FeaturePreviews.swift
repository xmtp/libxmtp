import ExampleCore
import Foundation
import SwiftUI

/// Preview-only in-memory services. No SDK, Keychain, network or disk access.
private enum PreviewScreen { case connect, handle, changeHandle, chats, requests, newMessage }

@MainActor
private final class FeaturePreviewFixture {
	let services: PreviewServices
	let conversations: ConversationsStore
	let requests: RequestsStore
	let handle: HandleStore
	let newMessage: NewConversationStore
	let connect: ConnectStore

	init(screen: PreviewScreen) {
		let services = PreviewServices(changingHandle: screen == .changeHandle)
		let fence = SessionFence()
		let profileID = UUID()
		let key = fence.replace(profileID) ?? SessionKey(profileID: profileID, generation: 0)
		let routes = RouteOwner()
		routes.replace(screen == .requests ? .requests : .chats)
		let origin = DestinationOrigin(session: key, token: routes.token)
		self.services = services
		conversations = ConversationsStore(session: key, fence: fence, routes: routes,
		                                   queries: PreviewConversationQueries(), preferences: services,
		                                   ownInboxID: "preview-own", consent: screen == .requests ? .unknown : .allowed)
		requests = RequestsStore(session: key, fence: fence, routes: routes,
		                         queries: PreviewConversationQueries(), preferences: services)
		handle = HandleStore(account: services, mode: screen == .changeHandle ? .change : .choose,
		                     origin: origin, admit: { routes.admit($0, fence: fence) }, finish: {})
		newMessage = NewConversationStore(queries: services, origin: origin,
		                                  admit: { routes.admit($0, fence: fence) }, finish: { _ in })
		newMessage.send(.group(true))
		newMessage.send(.select(Recipient(inboxID: "marco-inbox", handle: "marco#0412")))
		let session = SessionController(fence: fence, secrets: services, files: services, factory: services)
		connect = ConnectStore(probe: PreviewProbe(), session: session, owner: routes,
		                       defaultURL: "https://staging.xmtp.example")
		connect.send(.nameChanged("Staging server"))
	}
}

private struct FeaturePreview: View {
	let screen: PreviewScreen
	@State private var fixture: FeaturePreviewFixture

	init(_ screen: PreviewScreen) {
		self.screen = screen
		_fixture = State(initialValue: FeaturePreviewFixture(screen: screen))
	}

	var body: some View {
		NavigationStack {
			switch screen {
			case .connect: ConnectView(store: fixture.connect)
			case .handle, .changeHandle: HandleView(store: fixture.handle)
			case .chats:
				ChatsView(store: fixture.conversations, decisions: fixture.requests,
				          onNavigate: { _ in }, serverName: "Staging server")
			case .requests:
				RequestsView(store: fixture.conversations, decisions: fixture.requests, onNavigate: { _ in })
			case .newMessage: NewConversationView(store: fixture.newMessage)
			}
		}
		.tint(Theme.tint)
		.messengerMotion()
	}
}

private struct PreviewProbe: BackendProbe {
	func probe(_: BackendURL) async throws -> BackendProbeResult {
		BackendProbeResult(requiresToken: false)
	}
}

private struct PreviewConversationQueries: ConversationQueries {
	func list(consent: ConversationConsent, limit: Int) async throws -> [ConversationRecord] {
		let date = Date(timeIntervalSince1970: 1_791_913_260)
		let records: [ConversationRecord] = consent == .unknown ? [
			.init(id: "jun", kind: .dm, peerInboxID: "jun-inbox", peerHandle: "jun_park", consent: .unknown,
			      preview: "Hi! I found your inbox ID in the interop notes. Can we test file uploads together?", unread: 2,
			      lastActivity: date),
			.init(id: "faucet", kind: .group, name: "Testnet faucet", consent: .unknown,
			      preview: "faucet-bot: Welcome. Ask here for test funds.", lastActivity: date.addingTimeInterval(-86400)),
			.init(id: "9c0de1f27b44", kind: .dm, peerInboxID: "9c0de1f27b44", consent: .unknown,
			      preview: "IMG_0098.jpeg", lastActivity: date.addingTimeInterval(-172_800)),
		] : [
			.init(
				id: "protocol",
				kind: .group,
				name: "Protocol design",
				preview: "priya: Pushed the key package rotation. Can you check the commit before lunch?",
				unread: 3,
				lastActivity: date,
			),
			.init(
				id: "maya",
				kind: .dm,
				peerInboxID: "maya-inbox",
				peerHandle: "maya",
				preview: "You: Sounds good. I will retry the upload after lunch.",
				lastActivity: date.addingTimeInterval(-1740),
			),
			.init(
				id: "release",
				kind: .group,
				name: "Release crew",
				preview: "marco#0412: Tagging 1.4.0-rc2 now.",
				lastActivity: date.addingTimeInterval(-4260),
			),
			.init(
				id: "lena",
				kind: .dm,
				peerInboxID: "lena-inbox",
				peerHandle: "lena#2207",
				preview: "IMG_2041.jpeg",
				lastActivity: date.addingTimeInterval(-86400),
			),
			.init(
				id: "hike",
				kind: .group,
				name: "Weekend hike",
				preview: "You: I will bring the trail map and the spare battery.",
				lastActivity: date.addingTimeInterval(-86400),
			),
		]
		return Array(records.prefix(limit))
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		record
	}

	func setConsent(id _: String, consent _: ConversationConsent) async throws {}
}

@MainActor
private final class PreviewServices: ConversationPreferences, NewConversationQueries, HandleAccount,
	SecretStore, ProfileFileStore, SessionFactory, SessionClient
{
	let changingHandle: Bool
	init(changingHandle: Bool) {
		self.changingHandle = changingHandle
	}

	var dismissedRequests: Set<String> = []
	func alias(for id: String) -> String? {
		id == "maya" ? "Maya" : nil
	}

	func muted(for logicalKey: String) -> Bool {
		logicalKey == "release"
	}

	func marker(for _: String) -> Int64 {
		0
	}

	func dismissRequest(_ id: String) throws {
		dismissedRequests.insert(id)
	}

	func people() async throws -> [Recipient] {
		[Recipient(inboxID: "marco-inbox", handle: "marco#0412"),
		 Recipient(inboxID: "priya-inbox", handle: "priya"),
		 Recipient(inboxID: "sam-inbox", handle: "sam-k"),
		 Recipient(inboxID: "lena-inbox", handle: "lena#2207"),
		 Recipient(inboxID: "0f3e8b21c5aa", handle: nil)]
	}

	func maximumMembers() async throws -> Int {
		250
	}

	func resolve(_ input: String) async throws -> Recipient {
		Recipient(inboxID: input, handle: input)
	}

	func create(recipients _: [String], group _: Bool, name _: String, description _: String,
	            adminOnly _: Bool) async throws -> String
	{
		"preview"
	}

	var minimum: Int {
		3
	}

	var claimTimes: [String: Date] {
		[:]
	}

	func handles() async throws -> OwnHandles {
		OwnHandles(current: changingHandle ? "nick#2341" : nil, others: changingHandle ? ["nick#0912", "nickm"] : [])
	}

	func availableHandles(name: String) async throws -> [String] {
		[name + "#2341", name + "#0912", name + "#5530"]
	}

	func claimHandle(handle _: String) async throws {}
	func recordClaim(handle _: String, at _: Date) throws {}
	func read(_: UUID, ref _: String) throws -> Data? {
		nil
	}

	func write(_: UUID, ref _: String, data _: Data) throws {}
	func remove(_: UUID, ref _: String) throws {}
	func removeAll(_: UUID) throws {}
	func randomBytes(count: Int) throws -> Data {
		Data(repeating: 0, count: count)
	}

	func loadIndex() throws -> SessionIndex {
		SessionIndex()
	}

	func saveIndex(_: SessionIndex) throws {}
	func paths(_: UUID) throws -> ProfilePaths {
		ProfilePaths(root: URL(fileURLWithPath: "/preview"))
	}

	func databaseExists(_: UUID) throws -> Bool {
		false
	}

	func removeDatabase(_: UUID) throws {}
	func removeExports(_: UUID) throws {}
	func removeProfile(_: UUID) throws {}
	// Matches the explicit security inputs of the SessionFactory protocol.
	// swiftlint:disable:next function_parameter_count
	func open(profile _: BackendProfile, paths _: ProfilePaths, wallet _: Data, databaseKey _: Data,
	          secrets _: any SecretStore, key _: SessionKey,
	          fence _: SessionFence) async throws -> any SessionClient
	{
		self
	}

	var inboxID: String {
		"preview-own"
	}

	var identity: String {
		"preview-identity"
	}

	func end() async throws {}
	func disableNotifications() async throws {}
}

#Preview("Connect · Light") { FeaturePreview(.connect).preferredColorScheme(.light) }
#Preview("Connect · Dark") { FeaturePreview(.connect).preferredColorScheme(.dark) }
#Preview("Connect · AX3") { FeaturePreview(.connect).environment(\.dynamicTypeSize, .accessibility3) }
#Preview("Choose handle · Light") { FeaturePreview(.handle).preferredColorScheme(.light) }
#Preview("Choose handle · Dark") { FeaturePreview(.handle).preferredColorScheme(.dark) }
#Preview("Choose handle · AX3") { FeaturePreview(.handle).environment(\.dynamicTypeSize, .accessibility3) }
#Preview("Change handle · Light") { FeaturePreview(.changeHandle).preferredColorScheme(.light) }
#Preview("Change handle · Dark") { FeaturePreview(.changeHandle).preferredColorScheme(.dark) }
#Preview("Change handle · AX3") { FeaturePreview(.changeHandle).environment(\.dynamicTypeSize, .accessibility3) }
#Preview("Chats · Light") { FeaturePreview(.chats).preferredColorScheme(.light) }
#Preview("Chats · Dark") { FeaturePreview(.chats).preferredColorScheme(.dark) }
#Preview("Chats · AX3") { FeaturePreview(.chats).environment(\.dynamicTypeSize, .accessibility3) }
#Preview("Requests · Light") { FeaturePreview(.requests).preferredColorScheme(.light) }
#Preview("Requests · Dark") { FeaturePreview(.requests).preferredColorScheme(.dark) }
#Preview("Requests · AX3") { FeaturePreview(.requests).environment(\.dynamicTypeSize, .accessibility3) }
#Preview("New message · Light") { FeaturePreview(.newMessage).preferredColorScheme(.light) }
#Preview("New message · Dark") { FeaturePreview(.newMessage).preferredColorScheme(.dark) }
#Preview("New message · AX3") { FeaturePreview(.newMessage).environment(\.dynamicTypeSize, .accessibility3) }
