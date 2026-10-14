import ExampleCore
import ExampleSDK
import Observation
import UIKit
import UserNotifications

@MainActor @Observable final class PushCoordinator {
	weak static var current: PushCoordinator?
	#if PUSH_ENABLED
		static let configured = true
	#else
		static let configured = false
	#endif
	private weak var graph: AppGraph?
	private(set) var controller: PushController?
	private var permission = false
	private var token: Data?
	private var tokenRegistrationFailed = false
	private var binding: UInt64 = 0
	private var boundPreferences: ProfilePreferences?
	private let publisher = LocalPushPublisher()
	var status: String {
		controller?.status ?? (Self.configured ? "Off" : "Off: APNs is not configured")
	}

	var enabled: Bool {
		controller?.appEnabled ?? false
	}

	init(graph: AppGraph) {
		self.graph = graph; Self.current = self
		graph.factory.notificationPreflight = { [weak self] id in
			guard Self.configured, let self, permission, let graph = self.graph else { return true }
			do {
				guard let key = try graph.secrets.read(id, ref: "preferences-key") else { return true }
				return try !ProfilePreferences(paths: graph.files.paths(id), secret: key).notificationsEnabled
			} catch { return true }
		}
		graph.session.beforeEnd = { [weak self] in await self?.close() }
	}

	func bind(preferences: ProfilePreferences) async {
		guard let graph, let info = graph.active, graph.session.pushSessionIsCurrent(info.key),
		      let client = graph.factory.client(for: info.key) else { return }
		if let controller,
		   controller.owner.session == info
		   .key
		{
			controller.replacePreferences(preferences); boundPreferences = preferences; return
		}
		binding &+= 1; let attempt = binding
		await controller?.close()
		guard binding == attempt, graph.session.pushSessionIsCurrent(info.key) else { return }
		let gateway = SDKNotificationGateway(client: client, limiter: graph.reads)
		let owner = PushOwner(session: info.key, installation: gateway.installation)
		let next = PushController(
			configured: Self.configured,
			owner: owner,
			gateway: gateway,
			preferences: preferences,
			publisher: publisher,
			current: { [weak graph] in
				guard let graph, graph.session.pushSessionIsCurrent(info.key) else { return nil }; return owner
			},
			permitted: { [weak self] in self?.permission == true },
			signedIn: { [weak graph] in graph?.session.pushSessionIsCurrent(info.key) == true },
			didCommit: { [weak graph] in
				guard let graph, graph.session.pushSessionIsCurrent(info.key) else { return }
				await graph.conversations?.refresh()
				guard graph.session.pushSessionIsCurrent(info.key) else { return }
				await graph.requestConversations?.refresh()
			},
		)
		controller = next; boundPreferences = preferences
		if let token {
			next.tokenChanged(token)
		}
		if tokenRegistrationFailed {
			next.registrationFailed()
		}
		await refreshPermission()
		guard binding == attempt, graph.session.pushSessionIsCurrent(info.key) else { await next.close(); return }
		next.refresh()
	}

	func close() async {
		binding &+= 1; let old = controller; controller = nil; boundPreferences = nil; publisher.removeAll(); await old?
			.close()
	}

	func refreshPermission() async {
		#if PUSH_ENABLED
			let settings = await UNUserNotificationCenter.current().notificationSettings()
			let previous = permission
			permission = settings.authorizationStatus == .authorized || settings.authorizationStatus == .provisional || settings
				.authorizationStatus == .ephemeral
			if previous != permission {
				controller?.permissionChanged()
			}
			if permission, enabled {
				tokenRegistrationFailed = false
				controller?.registrationStarted()
				UIApplication.shared.registerForRemoteNotifications()
			}
		#endif
	}

	func setEnabled(_ enabled: Bool) {
		guard Self.configured else { return }
		controller?.setEnabled(enabled)
		#if PUSH_ENABLED
			guard let graph, let key = graph.active?.key else { return }
			graph.session.scope.add { [weak self, weak graph] in
				guard let self, graph?.session.pushSessionIsCurrent(key) == true else { return }
				if enabled {
					do {
						_ = try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])
					} catch {}
				}
				await refreshPermission()
			}
		#endif
	}

	func tokenChanged(_ token: Data) {
		tokenRegistrationFailed = false; self.token = token; controller?.tokenChanged(token)
	}

	func registrationFailed() {
		tokenRegistrationFailed = true
		controller?.registrationFailed()
	}

	func beginPrivacyChange() {
		controller?.beginPrivacyChange()
	}

	func endPrivacyChange() {
		controller?.endPrivacyChange()
	}

	func invalidatePrivacy() {
		controller?.invalidatePrivacy()
	}

	func setConversationEnabled(_ enabled: Bool, id: String) {
		guard let graph, let controller else { return }
		let origin = DestinationOrigin(session: controller.owner.session, token: graph.routes.token)
		graph.session.scope.add { [weak graph] in
			_ = await controller.setConversationEnabled(
				enabled,
				id: id,
				admit: { guard let graph else { return false }; return graph.routes.admit(origin, fence: graph.fence) },
			)
		}
	}

	func receive(topic: String, sequence: String, completion: @escaping (UIBackgroundFetchResult) -> Void) {
		let lease = PushLease()
		let result = PushFetchCompletion(lease: lease, completion: completion)
		guard Self.configured,
		      let envelope = PushEnvelope(topic: topic, sequence: sequence) else { result.finish(.noData); return }
		result.deadline = Task { do { try await Task.sleep(for: .seconds(25)) } catch { return }; result.finish(.noData) }
		result.work = Task { [weak self] in
			guard let self, let graph else { result.finish(.noData); return }
			await refreshPermission()
			guard permission, lease.active, savedPushEnabled(graph), let info = await graph.session.restoreForPush(),
			      lease.active,
			      let prefs = loadPreferences(info), prefs.notificationsEnabled else { result.finish(.noData); return }
			await bind(preferences: prefs)
			guard lease.active, let controller else { result.finish(.noData); return }
			result.work = graph.session.scope.add {
				guard lease.active else { result.finish(.noData); return }
				let posted = await controller.receive(envelope, lease: lease)
				result.finish(posted ? .newData : .noData)
			}
		}
	}

	private func loadPreferences(_ info: SessionInfo) -> ProfilePreferences? {
		if controller?.owner.session == info.key, let boundPreferences {
			return boundPreferences
		}
		guard let graph else { return nil }
		do {
			guard let key = try graph.secrets.read(info.key.profileID, ref: "preferences-key") else { return nil }
			return try ProfilePreferences(paths: graph.files.paths(info.key.profileID), secret: key)
		} catch { return nil }
	}

	private func savedPushEnabled(_ graph: AppGraph) -> Bool {
		do {
			let index = try graph.files.loadIndex()
			guard index.signedIn, index.reset == nil, let id = index.activeID,
			      try graph.files.databaseExists(id),
			      let key = try graph.secrets.read(id, ref: "preferences-key") else { return false }
			return try ProfilePreferences(paths: graph.files.paths(id), secret: key).notificationsEnabled
		} catch { return false }
	}

	func tap(_ route: PushRoute) async {
		guard let graph else { return }
		await refreshPermission()
		guard let info = await graph.session.restoreForPush(), info.key.profileID == route.profileID,
		      let prefs = loadPreferences(info) else { return }
		await bind(preferences: prefs)
		guard let controller else { return }
		graph.session.scope.add {
			guard let route = await controller.tap(route), !Task.isCancelled else { return }
			await graph.session.restore()
			await graph.bindSession()
			guard graph.session.pushSessionIsCurrent(info.key), !Task.isCancelled else { return }
			graph.navigate(route.conversationID.map(Route.chat) ?? .chats)
		}
	}
}

/// Timer and work deliberately do not await each other: an SDK call may ignore cancellation.
@MainActor private final class PushFetchCompletion {
	let lease: PushLease
	private let completion: PushCompletion<UIBackgroundFetchResult>
	var deadline: Task<Void, Never>?
	var work: Task<Void, Never>?
	init(lease: PushLease, completion: @escaping (UIBackgroundFetchResult) -> Void) {
		self.lease = lease; self.completion = PushCompletion(
			lease: lease,
			callback: completion,
		)
	}

	func finish(_ value: UIBackgroundFetchResult) {
		guard completion.finish(value) else { return }
		deadline?.cancel(); work?.cancel(); deadline = nil; work = nil
	}
}

@MainActor final class LocalPushPublisher: PushPublisher {
	private let center = UNUserNotificationCenter.current()
	private let submission = PushSubmissionFence()
	func post(tag: String, route: PushRoute, completion: @escaping @MainActor (Bool) -> Void) {
		let content = UNMutableNotificationContent(); content.title = String(
			localized: "XMTP Messenger",
			table: "Push",
		); content
			.body = String(localized: "You got a message.", table: "Push"); content.sound = .default
		var info = ["push-profile": route.profileID.uuidString]
		if let id = route.conversationID {
			info["push-conversation"] = id
		}
		content.userInfo = info
		let expectedRevision = submission.revision
		Task { @MainActor in
			let pending = await center.pendingNotificationRequests()
			let delivered = await center.deliveredNotifications()
			guard submission.revision == expectedRevision else { completion(false); return }
			if pending.contains(where: { $0.identifier == tag }) ||
				delivered.contains(where: { $0.request.identifier == tag })
			{
				completion(true)
				return
			}
			let posted = await submission.submit(expectedRevision: expectedRevision, add: {
				try await center.add(UNNotificationRequest(identifier: tag, content: content, trigger: nil))
			}, remove: {
				center.removePendingNotificationRequests(withIdentifiers: [tag])
				center.removeDeliveredNotifications(withIdentifiers: [tag])
			})
			completion(posted)
		}
	}

	func remove(tag: String) {
		submission.invalidate()
		center.removePendingNotificationRequests(withIdentifiers: [tag]); center
			.removeDeliveredNotifications(withIdentifiers: [tag])
	}

	func removeAll() {
		submission.invalidate()
		center.removeAllPendingNotificationRequests(); center.removeAllDeliveredNotifications()
	}
}
