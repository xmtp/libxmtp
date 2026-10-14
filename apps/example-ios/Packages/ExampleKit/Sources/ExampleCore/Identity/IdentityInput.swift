import Foundation

public enum IdentityInput: Equatable, Sendable {
	case inbox(String), ethereum(String), handle(String)
	public static func parse(_ input: String) throws -> Self {
		let value = input.trimmingCharacters(in: .whitespacesAndNewlines)
		guard !value.isEmpty else { throw IdentityError.empty }
		let hex: (String)
			-> Bool = { $0.utf8.allSatisfy { (48 ... 57).contains($0) || (65 ... 70).contains($0) || (97 ... 102).contains($0) }
			}
		if value.lowercased()
			.hasPrefix("0x")
		{
			guard value.count == 42,
			      hex(String(value.dropFirst(2)))
			else { throw IdentityError.invalidAddress }; return .ethereum(value.lowercased())
		}
		if value.count == 64 {
			guard hex(value) else { throw IdentityError.invalidInbox }; return .inbox(value.lowercased())
		}
		let handle = HandleRules.normalize(value.hasPrefix("@") ? String(value.dropFirst()) : value)
		guard !handle.isEmpty,
		      HandleRules.problem(handle, minimum: 1) == nil
		else { throw IdentityError.invalidHandle }; return .handle(handle)
	}

	public func resolve(using queries: any IdentityQueries) async throws -> String {
		if case let .inbox(id) = self {
			return id
		}
		guard let inbox = try await queries.inboxID(for: self) else { throw notFound }; return inbox
	}

	public var notFound: IdentityError {
		if case .handle = self {
			.handleNotFound
		} else {
			.addressNotFound
		}
	}
}

public enum IdentityError: String, Error, Sendable {
	case empty = "Enter a handle, Ethereum address, or inbox ID."
	case invalidAddress = "Enter an Ethereum address with 0x followed by 40 hexadecimal characters."
	case invalidInbox = "An inbox ID must contain 64 hexadecimal characters."
	case invalidHandle = "Enter a handle such as nick or nick#2341, an Ethereum address, or an inbox ID."
	case handleNotFound = "No XMTP account has this handle."
	case addressNotFound = "This address has no XMTP account."
}

public protocol IdentityQueries: Sendable {
	func inboxID(for identity: IdentityInput) async throws -> String?
	func handles(inboxIDs: [String]) async throws -> [InboxIdentity]
}

public struct InboxIdentity: Equatable, Sendable {
	public let inboxID: String; public let handle: String?; public let otherHandles: [String]
	public init(inboxID: String, handle: String?, otherHandles: [String] = []) {
		self.inboxID = inboxID; self.handle = handle; self.otherHandles = otherHandles
	}
}
