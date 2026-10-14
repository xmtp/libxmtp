import Foundation

public struct BackendURL: Equatable, Sendable {
	public let value: String
	public let allowPrivateNetwork: Bool
	public init(_ input: String) throws {
		var text = input.trimmingCharacters(in: .whitespacesAndNewlines)
		while text.hasSuffix("/") {
			text.removeLast()
		}
		guard var parts = URLComponents(string: text), let scheme = parts.scheme?.lowercased(),
		      ["http", "https"].contains(scheme), let host = parts.host,
		      !host.isEmpty else { throw SessionError.message("Use a valid HTTP or HTTPS backend URL") }
		guard parts.user == nil, parts.password == nil, parts.query == nil, parts.fragment == nil,
		      !(parts.port.map { !(1 ... 65535).contains($0) } ?? false),
		      !(parts.url?.absoluteString.split(separator: "/").dropFirst().first?.hasSuffix(":") ?? false)
		else {
			throw SessionError.message("Use a backend URL without credentials, a query, a fragment, or an invalid port")
		}
		let local = ["localhost", "127.0.0.1", "::1", "[::1]"].contains(host.lowercased())
		guard scheme == "https" || local else { throw SessionError.message("Use HTTPS for a remote backend") }
		parts.scheme = scheme
		parts.host = host.lowercased()
		if parts.port == (scheme == "https" ? 443 : 80) {
			parts.port = nil
		}
		guard let canonical = parts.string else { throw SessionError.message("Use a valid HTTP or HTTPS backend URL") }
		value = canonical; allowPrivateNetwork = local
	}
}

public enum SessionError: Error, Equatable, Sendable { case message(String), stale, missingSecret }

extension SessionError: LocalizedError {
	public var errorDescription: String? {
		switch self {
		case let .message(text): text
		case .stale: "This session has ended."
		case .missingSecret: "Cannot restore this account. Try again."
		}
	}
}

public extension BackendURL { static func parse(_ value: String) -> BackendURL? {
	do { return try BackendURL(value) } catch { return nil }
} }
