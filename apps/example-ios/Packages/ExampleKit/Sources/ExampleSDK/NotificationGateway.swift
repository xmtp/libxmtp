import ExampleCore
import Foundation
import XmtpSdk

func pushConfiguration(token: String) -> NotificationConfig {
	NotificationConfig(
		channel: .apns(token: token),
		consentStates: [.allowed, .unknown],
		includeWelcomes: true,
		includeSyncGroups: false,
		includeCommits: false,
	)
}

func pushState(_ state: NotificationState) -> PushRegistrationState {
	switch state {
	case .disabled: .disabled
	case .enabled: .enabled
	case let .failed(error): .failed(pushFailure(error))
	}
}

func pushFailure(_ error: NotificationFailure) -> String {
	switch error {
	case .channelNotConfigured: "Backend APNs channel is not configured"
	case .permissionDenied: "Notification registration failed: permission denied"
	case .invalidArgument: "Notification registration failed: invalid argument"
	case .outOfRange: "Notification registration failed: out of range"
	case .unimplemented: "Notification registration failed: unsupported"
	}
}

@MainActor public final class SDKNotificationGateway: ExampleCore.NotificationGateway {
	private let client: SDKClient
	private let limiter: AsyncLimiter
	public var installation: String {
		PushEnvelope.hex(client.installationIdBytes())
	}

	public init(client: SDKClient, limiter: AsyncLimiter) {
		self.client = client; self.limiter = limiter
	}

	public func state() throws -> PushRegistrationState {
		try pushState(client.notificationState())
	}

	public func enable(token: String) async throws -> PushRegistrationState {
		do {
			return try await pushState(client.enableNotifications(config: pushConfiguration(token: token)))
		} catch XmtpError.ChannelNotConfigured {
			throw PushFailure("Backend APNs channel is not configured")
		} catch {
			throw PushFailure("Notification registration failed. Try again.")
		}
	}

	public func disable() async throws {
		try await client.disableNotifications()
	}

	public func conversation(id: String) async throws -> PushConversation? {
		guard let value = try await limiter.withPermit({ try await self.client.conversations.getById(id: id) })
		else { return nil }
		let state: ConversationState
		let logical: String
		switch value {
		case let .group(group): state = try await limiter.withPermit { try await group.state().common }; logical = value.id()
		case let .dm(dm):
			state = try await limiter.withPermit { try await dm.state() }
			guard let peer = try await limiter.withPermit({ try await dm.peerInboxId() }) else { return nil }
			logical = "dm:" + [client.inboxId(), peer].sorted().joined(separator: ":")
		}
		return PushConversation(
			id: value.id(),
			logicalKey: logical,
			admitted: state.isActive && state.consentState != .denied && state.notificationsEnabled,
		)
	}

	public func setEnabled(_ enabled: Bool, conversationID: String) async throws {
		guard let value = try await client.conversations.getById(id: conversationID)
		else { throw PushFailure("Conversation is unavailable.") }
		try await value.setNotifications(value: enabled ? .enabled : .disabled)
	}
}
