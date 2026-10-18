import ExampleCore
import ExampleSDK
import Observation
import SwiftUI

@MainActor @Observable final class AppGraph {
	let routes = RouteOwner()
	let fence = SessionFence()
	let factory = SDKSessionFactory()
	let secrets = KeychainSecretStore()
	let files: ProfileFiles
	let session: SessionController
	let reads = AsyncLimiter(4)
	var push: PushCoordinator?
	var connect: ConnectStore
	var conversations: ConversationsStore?
	var requestConversations: ConversationsStore?
	var requests: RequestsStore?
	var handles: HandleStore?
	var newConversation: NewConversationStore?
	var preferences: ProfilePreferences?
	var identityDirectory: IdentityDirectory?
	var consentRevision: UInt64 = 0
	var messageObservers: [UUID: @MainActor (String) async -> Void] = [:]
	var messageDeletionObservers: [UUID: @MainActor (String, String, SessionKey) -> Void] = [:]
	var isRestoring = true
	var currentHandle: String?
	var otherHandles: [String] = []
	var error: String?
	var appearance: String
	private var bound: SessionKey?
	private var sceneActive = true
	private var messageSource: (key: SessionKey, readers: SessionReaders)?
	private var messageRun: Task<Void, Never>?
	private var stoppingMessageReader: Task<Void, Never>?
	init() throws {
		files = try ProfileFiles()
		session = SessionController(fence: fence, secrets: secrets, files: files, factory: factory)
		connect = ConnectStore(probe: SDKBackendProbe(), session: session, owner: routes)
		// Session restore owns unreadable-index recovery; this is temporary UI styling only.
		appearance = "system"
		push = PushCoordinator(graph: self)
	}

	var serverName: String {
		preferences?.serverLabel ?? active?.profile.name ?? active?.profile.url ?? ""
	}

	var active: SessionInfo? {
		if case let .active(value) = session.state {
			value
		} else {
			nil
		}
	}

	func restore() async {
		defer { isRestoring = false }
		do { appearance = try files.loadIndex().appearance } catch {
			// The session restore below presents its existing recovery state without changing the index.
		}
		await session.restore(); await bindSession()
	}

	func bindSession() async {
		guard !session.backgroundOnly else { return }
		guard let info = active else {
			if session.state == .signedOut {
				do { appearance = try files.loadIndex().appearance } catch {
					// This updates styling only; connection still validates the stored index.
				}
			}
			clearSessionViews(); return
		}
		guard bound != info.key, factory.client(for: info.key) != nil else { return }
		bound = info.key
		do {
			appearance = try files.loadIndex().appearance
			try removeAbandonedAttachments(info)
			let prefs = try loadPreferences(info)
			guard await fence.afterCurrent(info.key, operation: { await push?.bind(preferences: prefs) }),
			      bound == info.key, active?.key == info.key else { return }
			preferences = prefs
			installStores(info, preferences: prefs)
			let initialRoute = session.scope.add { [weak self] in
				await self?.chooseInitialRoute(info, preferences: prefs)
			}
			await initialRoute.value
		} catch {
			guard fence.accepts(info.key), bound == info.key else { return }
			bound = nil; self.error = "Cannot open this account's preferences. Try again."
		}
	}

	private func clearSessionViews() {
		guard bound != nil, session.state != .resetIncomplete else { return }
		bound = nil; conversations = nil; requestConversations = nil; requests = nil
		handles = nil; newConversation = nil; preferences = nil; identityDirectory = nil; messageObservers
			.removeAll(); messageDeletionObservers.removeAll()
		currentHandle = nil; otherHandles = []; messageSource = nil; messageRun = nil
		routes.replace(.connect)
	}

	private func removeAbandonedAttachments(_ info: SessionInfo) throws {
		try files.removeExports(info.key.profileID)
		let temporary = try files.paths(info.key.profileID).root.appendingPathComponent("temp", isDirectory: true)
		try AttachmentStagingFiles(temporaryDirectory: temporary).removeAbandoned()
	}

	private func loadPreferences(_ info: SessionInfo) throws -> ProfilePreferences {
		guard let secret = try secrets.read(info.key.profileID, ref: "preferences-key") else {
			throw CocoaError(.fileReadUnknown)
		}
		let prefs = try ProfilePreferences(paths: files.paths(info.key.profileID), secret: secret)
		if let name = info.profile.name {
			try prefs.setServerLabel(name)
		}
		return prefs
	}

	private func installStores(_ info: SessionInfo, preferences prefs: ProfilePreferences) {
		guard let client = factory.client(for: info.key) else { return }
		let identities = SDKIdentityQueries(client: client, limiter: reads)
		identityDirectory = IdentityDirectory(queries: identities, valid: { [weak self] in
			self?.fence.accepts(info.key) == true
		})
		let privacyChange: @MainActor @Sendable (Bool) -> Void = { [weak controller = push?.controller] beginning in
			if beginning {
				controller?.beginPrivacyChange()
			} else {
				controller?.endPrivacyChange()
			}
		}
		let queries = SDKConversationQueries(client: client, limiter: reads, identityDirectory: identityDirectory,
		                                     privacyChange: privacyChange)
		conversations = ConversationsStore(session: info.key, fence: fence, routes: routes, queries: queries,
		                                   preferences: prefs, ownInboxID: info.profile.inboxID ?? "")
		requestConversations = ConversationsStore(session: info.key, fence: fence, routes: routes, queries: queries,
		                                          preferences: prefs, ownInboxID: info.profile.inboxID ?? "",
		                                          consent: .unknown)
		requests = RequestsStore(session: info.key, fence: fence, routes: routes, queries: queries, preferences: prefs)
		startReaders(info, queries: queries)
		if let conversations {
			session.scope.addCleanup { await conversations.close() }
		}
		if let requests {
			session.scope.addCleanup { await requests.close() }
		}
		if let requestConversations {
			session.scope.addCleanup { await requestConversations.close() }
		}
	}

	private func startReaders(_ info: SessionInfo, queries: SDKConversationQueries) {
		let readers = queries.readers(projectMessage: { [weak self] id in
			await self?.projectMessage(id, session: info.key)
		}, projectConversation: { [weak self] _ in
			await self?.reloadConversations(session: info.key)
		}, projectEvent: { [weak self] event in
			await self?.projectEvent(event, session: info.key)
		}, connection: { [weak self] banner in
			Task { @MainActor in
				guard let self, self.fence.accepts(info.key) else { return }
				self.conversations?.send(.connection(banner))
			}
		})
		let ownedConversations = conversations
		session.scope.add { [weak self] in
			await self?.retrying(info.key) { try await readers.runUpdates() }
			// Close the stores only after the message reader can no longer project into them.
			await self?.messageRun?.value
			await ownedConversations?.close()
		}
		// The previous session's scope already cancelled and awaited its message reader.
		messageSource = (info.key, readers); messageRun = nil
		runMessageReader()
	}

	/// The default message reader runs only while a scene is active. Closing it releases its consumer lease. A process
	/// that iOS suspends, or that the user kills from the app switcher, would otherwise keep the lease, and the next
	/// launch's reader fails with ConsumerOwned until the lease expires.
	func setSceneActive(_ active: Bool) async {
		sceneActive = active
		guard active else { await pauseMessageReader(); return }
		await stoppingMessageReader?.value
		runMessageReader()
	}

	private func runMessageReader() {
		guard sceneActive, messageRun == nil, let (key, readers) = messageSource, fence.accepts(key) else { return }
		messageRun = session.scope.add { [weak self] in
			await self?.retrying(key) { try await readers.runMessages() }
		}
	}

	private func pauseMessageReader() async {
		guard let run = messageRun else { await stoppingMessageReader?.value; return }
		messageRun = nil; stoppingMessageReader = run
		run.cancel(); await run.value
	}

	private func retrying(_ key: SessionKey, _ read: @escaping @Sendable () async throws -> Void) async {
		while !Task.isCancelled, fence.accepts(key) {
			do { try await read(); break } catch is CancellationError { break } catch {
				guard fence.accepts(key) else { break }
				conversations?.send(.connection("Disconnected. Retrying…"))
				do { try await Task.sleep(for: .seconds(1)) } catch { break }
			}
		}
	}

	private func chooseInitialRoute(_ info: SessionInfo, preferences prefs: ProfilePreferences) async {
		guard let client = factory.client(for: info.key) else { return }
		let account = SDKHandleAccount(client: client, limiter: reads, claimTimes: prefs.claimTimes,
		                               recordClaim: { handle, date in try prefs.recordClaim(handle, at: date) })
		do {
			let own = try await account.handles()
			guard fence.accepts(info.key) else { return }
			currentHandle = own.current; otherHandles = own.others
			identityDirectory?.merge([InboxIdentity(inboxID: info.profile.inboxID ?? "", handle: own.current,
			                                        otherHandles: own.others)])
			if own.current == nil, !prefs.skippedHandle {
				routes.replace(.chooseHandle); prepareHandle(.choose)
			} else {
				routes.replace(.chats)
			}
		} catch {
			guard fence.accepts(info.key) else { return }; routes.replace(.chats)
		}
	}

	private func projectMessage(_ id: String, session key: SessionKey) async {
		guard fence.accepts(key) else { return }
		for observer in messageObservers.values {
			await observer(id)
		}
		await reloadConversations(session: key)
	}

	private func reloadConversations(session key: SessionKey) async {
		guard fence.accepts(key), let conversations else { return }
		await conversations.refresh()
		await requestConversations?.refresh()
	}

	private func projectEvent(_ event: SessionEvent, session key: SessionKey) async {
		guard fence.accepts(key) else { return }
		if case let .messageDeleted(conversationID, messageID, _) = event {
			ChatServices.messageDeleted(conversationID: conversationID, messageID: messageID, session: key)
			for observer in messageDeletionObservers.values {
				observer(conversationID, messageID, key)
			}
			await projectMessage(conversationID, session: key)
			return
		}
		if case .changed = event {
			consentRevision &+= 1
		}
		if case let .handleChanged(inboxID, handle, otherHandles) = event {
			identityDirectory?.handleChanged(inboxID: inboxID, handle: handle, otherHandles: otherHandles)
			if inboxID == active?.profile.inboxID {
				currentHandle = handle
				self.otherHandles = identityDirectory?.identities[inboxID]?.otherHandles ?? self.otherHandles
			}
		}
		await reloadConversations(session: key)
	}
}
