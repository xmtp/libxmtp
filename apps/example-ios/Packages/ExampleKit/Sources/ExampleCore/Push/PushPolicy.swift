import Foundation

public struct PushEnvelope: Equatable, Sendable {
	public enum Kind: Sendable { case group, welcome }
	public let topic: String
	public let sequence: UInt64
	public let kind: Kind
	public let identifier: String
	public var tag: String {
		topic + ":" + String(sequence)
	}

	public init?(topic: String, sequence: String) {
		guard !sequence.isEmpty, sequence.utf8.allSatisfy({ (48 ... 57).contains($0) }), let number = UInt64(sequence),
		      let bytes = Data(base64Encoded: topic), bytes.base64EncodedString() == topic else { return nil }
		// Group and welcome topics can both be 33 bytes, so the kind byte decides:
		// 0x00 carries a 32-byte group id (or a 16-byte legacy id), 0x01 a 32-byte installation key.
		switch bytes.first {
		case 0? where bytes.count == 33 || bytes.count == 17:
			kind = .group
		case 1? where bytes.count == 33:
			kind = .welcome
		default:
			return nil
		}
		self.topic = topic; self.sequence = number; identifier = Self.hex(bytes.dropFirst())
	}

	public static func hex(_ bytes: some Sequence<UInt8>) -> String {
		bytes.map { String(format: "%02x", $0) }.joined()
	}
}

public struct PushOwner: Equatable, Sendable {
	public let session: SessionKey
	public let installation: String
	public init(session: SessionKey, installation: String) {
		self.session = session; self.installation = installation
	}
}

public struct PushConversation: Equatable, Sendable {
	public let id: String
	public let logicalKey: String
	public let admitted: Bool
	public init(id: String, logicalKey: String, admitted: Bool) {
		self.id = id; self.logicalKey = logicalKey; self.admitted = admitted
	}
}

public struct PushRoute: Equatable, Codable, Sendable {
	public let profileID: UUID
	public let conversationID: String?
	public init(profileID: UUID, conversationID: String?) {
		self.profileID = profileID; self.conversationID = conversationID
	}
}

public enum PushRegistrationState: Equatable, Sendable { case disabled, enabled, failed(String) }
public enum PushRegistrationAction: Equatable, Sendable { case none, disable, enable }
public struct PushRegistrationInput: Sendable {
	public var configured: Bool
	public var signedIn: Bool
	public var permission: Bool
	public var appEnabled: Bool
	public var token: String?
	public init(configured: Bool, signedIn: Bool, permission: Bool, appEnabled: Bool, token: String?) {
		self.configured = configured; self.signedIn = signedIn; self.permission = permission; self
			.appEnabled = appEnabled; self.token = token
	}

	public func action(registeredToken: String?, sdkEnabled: Bool) -> PushRegistrationAction {
		guard configured else { return .none }
		guard signedIn, permission, appEnabled, let token, !token.isEmpty else {
			return sdkEnabled || registeredToken != nil ? .disable : .none
		}
		return !sdkEnabled || registeredToken != token ? .enable : .none
	}
}

@MainActor public protocol NotificationGateway: AnyObject {
	var installation: String { get }
	func state() throws -> PushRegistrationState
	func enable(token: String) async throws -> PushRegistrationState
	func disable() async throws
	func conversation(id: String) async throws -> PushConversation?
	func setEnabled(_ enabled: Bool, conversationID: String) async throws
}

@MainActor public protocol PushPreferences: AnyObject {
	var notificationsEnabled: Bool { get }
	func setNotificationsEnabled(_ enabled: Bool) throws
	func muted(for logicalKey: String) -> Bool
	func setMute(_ muted: Bool, for logicalKey: String) throws
	func hasPosted(_ tag: String) -> Bool
	func recordPosted(_ tag: String) throws
}

/// A lease is revoked by timeout before late work can post or route.
@MainActor public final class PushLease {
	public private(set) var active = true
	public init() {}
	public func cancel() {
		active = false
	}
}

/// The timeout and work race through this one main-actor completion owner.
@MainActor public final class PushCompletion<Value> {
	public let lease: PushLease
	private var callback: ((Value) -> Void)?
	public init(lease: PushLease, callback: @escaping (Value) -> Void) {
		self.lease = lease; self.callback = callback
	}

	@discardableResult public func finish(_ value: Value) -> Bool {
		guard let callback else { return false }
		self.callback = nil; lease.cancel(); callback(value); return true
	}
}
