import Foundation

/// Delivery channel for this installation.
public enum NotificationChannel {
	case apns(token: String)
	case fcm(token: String)
	case http(url: String, signingKey: Data)

	var toFFI: FfiNotificationChannel {
		switch self {
		case let .apns(token): .apns(token: token)
		case let .fcm(token): .fcm(token: token)
		case let .http(url, signingKey): .http(url: url, signingKey: signingKey)
		}
	}
}

/// Notification delivery and conversation rules.
public struct NotificationConfig {
	public var channel: NotificationChannel
	public var consentStates: [ConsentState]
	public var includeWelcomes: Bool
	public var includeSyncGroups: Bool
	public var includeCommits: Bool

	public init(
		channel: NotificationChannel,
		consentStates: [ConsentState] = [.allowed],
		includeWelcomes: Bool = true,
		includeSyncGroups: Bool = false,
		includeCommits: Bool = false
	) {
		self.channel = channel
		self.consentStates = consentStates
		self.includeWelcomes = includeWelcomes
		self.includeSyncGroups = includeSyncGroups
		self.includeCommits = includeCommits
	}

	var toFFI: FfiNotificationConfig {
		FfiNotificationConfig(
			channel: channel.toFFI,
			consentStates: consentStates.toFFI,
			includeWelcomes: includeWelcomes,
			includeSyncGroups: includeSyncGroups,
			includeCommits: includeCommits
		)
	}
}

/// Reset to the configured consent rules with `.default`.
public enum NotificationOverride {
	case enabled, disabled, `default`

	var toFFI: FfiNotificationOverride {
		switch self {
		case .enabled: .enabled
		case .disabled: .disabled
		case .default: .default
		}
	}
}

/// A notification failure with a stable error code.
public struct NotificationError: Error, Equatable {
	public let code: String

	init(_ failure: FfiNotificationFailure) {
		let name = switch failure {
		case .permissionDenied: "PermissionDenied"
		case .invalidArgument: "InvalidArgument"
		case .outOfRange: "OutOfRange"
		case .unimplemented: "Unimplemented"
		case .channelNotConfigured: "ChannelNotConfigured"
		}
		code = "NotificationError::\(name)"
	}

	private init(code: String) {
		self.code = code
	}

	static func from(_ error: Error) -> Error {
		guard case let FfiError.Error(message) = error,
		      message.hasPrefix("[NotificationError::"),
		      let end = message.firstIndex(of: "]")
		else { return error }
		return NotificationError(code: String(message[message.index(after: message.startIndex) ..< end]))
	}
}

/// Local notification state. Reading it makes no backend request.
public enum NotificationState: Equatable {
	case disabled, enabled
	case failed(NotificationError)

	init(_ value: FfiNotificationState) {
		switch value {
		case .disabled: self = .disabled
		case .enabled: self = .enabled
		case let .failed(error): self = .failed(NotificationError(error))
		}
	}
}
