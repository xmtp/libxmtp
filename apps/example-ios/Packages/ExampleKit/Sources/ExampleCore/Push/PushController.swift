import Foundation
import Observation

@MainActor public protocol PushPublisher: AnyObject, Sendable {
	/// Enqueue this unique submission synchronously; deliver completion on the main actor.
	func post(submissionID: UUID, tag: String, route: PushRoute, completion: @escaping @MainActor (Bool) -> Void)
	/// Remove only this submission, including when a newer post has the same logical tag.
	func remove(submissionID: UUID)
	func remove(conversationID: String)
	func removeAll()
}

@MainActor @Observable public final class PushController {
	public private(set) var status: String
	public private(set) var appEnabled = false
	public let configured: Bool
	private let current: () -> PushOwner?
	private let permitted: () -> Bool
	private let signedIn: () -> Bool
	private let gateway: any NotificationGateway
	private var preferences: any PushPreferences
	private let publisher: any PushPublisher
	private let didCommit: @MainActor () async -> Void
	private var token: String?
	private var tokenRegistrationFailed = false
	private var registeredToken: String?
	private var registration: Task<Void, Never>?
	private var stopped = false
	private var optedOut = false
	private var settingSaveFailed = false
	private var explicitRegistrationRetry = false
	private var privacyRevision: UInt64 = 0
	private var pendingPrivacyWrites = 0
	private let conversationPrivacy = PushConversationPrivacy()
	private var posting: Set<String> = []
	public let owner: PushOwner

	public init(configured: Bool, owner: PushOwner, gateway: any NotificationGateway, preferences: any PushPreferences,
	            publisher: any PushPublisher, current: @escaping () -> PushOwner?, permitted: @escaping () -> Bool,
	            signedIn: @escaping () -> Bool, didCommit: @escaping @MainActor () async -> Void = {})
	{
		self.configured = configured; self.owner = owner; self.gateway = gateway; self.preferences = preferences
		self.publisher = publisher; self.current = current; self.permitted = permitted; self.signedIn = signedIn; self
			.didCommit = didCommit
		status = configured ? "Off" : "Off: APNs is not configured"
		appEnabled = configured && notificationsEnabled
	}

	public func replacePreferences(_ preferences: any PushPreferences) {
		guard self.preferences !== preferences else { return }
		invalidatePrivacy(); self.preferences = preferences; appEnabled = configured && notificationsEnabled
	}

	public func beginPrivacyChange() {
		pendingPrivacyWrites += 1; invalidatePrivacy()
	}

	public func endPrivacyChange() {
		pendingPrivacyWrites = max(0, pendingPrivacyWrites - 1); privacyRevision &+= 1
	}

	public func invalidatePrivacy() {
		privacyRevision &+= 1; publisher.removeAll()
	}

	private var valid: Bool {
		!stopped && current() == owner
	}

	private var notificationsEnabled: Bool {
		preferences.notificationsEnabled && !optedOut
	}

	private var enabled: Bool {
		configured && valid && signedIn() && permitted() && notificationsEnabled
	}

	public func tokenChanged(_ data: Data) {
		guard valid else { return }
		tokenRegistrationFailed = false; token = PushEnvelope.hex(data)
		do { status = try statusText(gateway.state()) } catch {
			status = "Notification registration failed. Try again."
		}
		refresh()
	}

	public func registrationStarted() {
		guard configured, valid else { return }
		tokenRegistrationFailed = false
		do { status = try statusText(gateway.state()) } catch {
			status = "Notification registration failed. Try again."
		}
		refresh()
	}

	public func registrationFailed() {
		guard configured, valid else { return }
		tokenRegistrationFailed = true
		status = "APNs registration failed. Try again."
	}

	public func permissionChanged() {
		invalidatePrivacy(); refresh()
	}

	public func refresh() {
		guard configured, valid else { return }
		appEnabled = notificationsEnabled
		let previous = registration
		registration = Task { [weak self] in
			await previous?.value
			guard let self, valid else { return }
			await reconcile()
		}
	}

	private func reconcile() async {
		do {
			let result = try await reconcileRegistration()
			guard valid else { return }
			status = statusText(result)
		} catch {
			guard valid else { return }
			status = (error as? PushFailure)?.message ?? "Notification registration failed. Try again."
		}
	}

	private func reconcileRegistration() async throws -> PushRegistrationState {
		let state = try gateway.state()
		let input = PushRegistrationInput(configured: configured, signedIn: signedIn(), permission: permitted(),
		                                  appEnabled: notificationsEnabled, token: token)
		let action = input.action(registeredToken: registeredToken, sdkEnabled: state == .enabled)
		if case .failed = state, action == .enable, !explicitRegistrationRetry {
			return state
		}
		switch action {
		case .enable:
			guard let token else { return state }
			explicitRegistrationRetry = false
			let result = try await gateway.enable(token: token)
			if valid, result == .enabled {
				registeredToken = token
			}
			return result
		case .disable:
			explicitRegistrationRetry = false
			try await gateway.disable(); registeredToken = nil; return .disabled
		case .none:
			if state == .enabled {
				explicitRegistrationRetry = false
			}
			return state
		}
	}

	public func setEnabled(_ value: Bool) {
		guard configured, valid else { return }
		invalidatePrivacy()
		// An explicit opt-out stops admission even when its durable write fails.
		if !value {
			optedOut = true; explicitRegistrationRetry = false
		}
		do {
			try preferences.setNotificationsEnabled(value)
			optedOut = !value; settingSaveFailed = false; explicitRegistrationRetry = value
		} catch { settingSaveFailed = true }
		refresh()
		if settingSaveFailed {
			status = "Cannot save notification setting. Try again."
		}
	}

	@discardableResult public func setConversationEnabled(
		_ value: Bool,
		id: String,
		admit: @MainActor () -> Bool = { true },
	) async -> Bool {
		guard configured, valid, admit() else { return false }
		conversationPrivacy.begin(id); publisher.remove(conversationID: id)
		defer { conversationPrivacy.end(id) }
		do {
			guard let conversation = try await gateway.conversation(id: id), valid, admit() else { return false }
			try await gateway.setEnabled(value, conversationID: id)
			// Once the SDK accepts the write, its durable preference belongs to the session,
			// even if the originating screen was left while the write was in flight.
			guard valid else { return false }
			try preferences.setMute(!value, for: conversation.logicalKey)
			await didCommit()
			refresh()
			return true
		} catch {
			if valid, admit() {
				status = "Cannot change conversation notifications. Try again."
			}; return false
		}
	}

	public func close() async {
		stopped = true; invalidatePrivacy()
		await registration?.value
		// This is deliberately after all enables, even when their caller was cancelled.
		if configured {
			do { try await gateway.disable() } catch {}
		}
		registeredToken = nil
	}

	private func admittedConversation(_ id: String) async throws -> PushConversation? {
		guard let value = try await gateway.conversation(id: id), enabled, value.admitted,
		      !preferences.muted(for: value.logicalKey) else { return nil }
		return value
	}

	public func receive(_ envelope: PushEnvelope, lease: PushLease) async -> Bool {
		let conversationID = envelope.kind == .group ? envelope.identifier : nil
		let conversationRevision = conversationPrivacy.revision(for: conversationID)
		guard conversationPrivacy.admits(conversationID, revision: conversationRevision), lease.active, enabled,
		      pendingPrivacyWrites == 0, !posting.contains(envelope.tag),
		      !preferences.hasPosted(envelope.tag) else { return false }
		let revision = privacyRevision
		do {
			guard let route = try await route(for: envelope, lease: lease) else { return false }
			guard conversationPrivacy.admits(conversationID, revision: conversationRevision),
			      lease.active, enabled, pendingPrivacyWrites == 0, revision == privacyRevision,
			      !posting.contains(envelope.tag), !preferences.hasPosted(envelope.tag) else { return false }
			posting.insert(envelope.tag)
			let waiter = PushPostWaiter()
			let submissionID = UUID()
			let publisher = publisher
			return await withTaskCancellationHandler {
				await withCheckedContinuation { continuation in
					waiter.install(continuation)
					publisher.post(submissionID: submissionID, tag: envelope.tag, route: route) { [weak self, publisher] success in
						guard let self else { publisher.remove(submissionID: submissionID); waiter.finish(false); return }
						posting.remove(envelope.tag)
						var keep = success && lease.active && enabled && pendingPrivacyWrites == 0 && revision == privacyRevision &&
							conversationPrivacy.admits(conversationID, revision: conversationRevision)
						if keep {
							do { try preferences.recordPosted(envelope.tag) } catch { keep = false }
						}
						if !keep {
							publisher.remove(submissionID: submissionID)
						}
						waiter.finish(keep)
					}
				}
			} onCancel: {
				Task { @MainActor [publisher] in
					lease.cancel(); publisher.remove(submissionID: submissionID)
					waiter.finish(false)
				}
			}
		} catch { return false }
	}

	private func route(for envelope: PushEnvelope, lease: PushLease) async throws -> PushRoute? {
		switch envelope.kind {
		case .group:
			guard let first = try await admittedConversation(envelope.identifier), lease.active,
			      let fresh = try await admittedConversation(envelope.identifier), first.id == fresh.id else { return nil }
			return PushRoute(profileID: owner.session.profileID, conversationID: fresh.id)
		case .welcome:
			guard envelope.identifier == owner.installation else { return nil }
			return PushRoute(profileID: owner.session.profileID, conversationID: nil)
		}
	}

	public func tap(_ route: PushRoute) async -> PushRoute? {
		let conversationRevision = conversationPrivacy.revision(for: route.conversationID)
		guard conversationPrivacy.admits(route.conversationID, revision: conversationRevision), enabled,
		      route.profileID == owner.session.profileID, pendingPrivacyWrites == 0 else { return nil }
		let revision = privacyRevision
		if let id = route.conversationID {
			do { guard try await admittedConversation(id) != nil else { return nil } } catch { return nil }
		}
		return enabled && pendingPrivacyWrites == 0 && revision == privacyRevision &&
			conversationPrivacy.admits(route.conversationID, revision: conversationRevision) ? route : nil
	}
}

private extension PushController {
	func statusText(_ result: PushRegistrationState) -> String {
		if settingSaveFailed {
			return "Cannot save notification setting. Try again."
		}
		guard notificationsEnabled else { return "Off" }
		guard permitted() else { return "Off: iOS permission is required" }
		if tokenRegistrationFailed {
			return "APNs registration failed. Try again."
		}
		guard token != nil else { return "Waiting for APNs token" }
		switch result {
		case .enabled: return "On"
		case .disabled: return "Off"
		case let .failed(reason): return reason
		}
	}
}

public struct PushFailure: Error, Sendable {
	public let message: String
	public init(_ message: String) {
		self.message = message
	}
}

@MainActor private final class PushPostWaiter {
	private var continuation: CheckedContinuation<Bool, Never>?
	private var result: Bool?
	func install(_ continuation: CheckedContinuation<Bool, Never>) {
		if let result {
			continuation.resume(returning: result)
		} else {
			self.continuation = continuation
		}
	}

	func finish(_ result: Bool) {
		guard self.result == nil else { return }
		self.result = result; continuation?.resume(returning: result); continuation = nil
	}
}
