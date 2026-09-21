import Foundation

/// A backend request credential. This is separate from the XMTP wallet signer.
public struct Credential: Sendable, CustomStringConvertible, CustomDebugStringConvertible {
	/// Header name. `nil` uses `authorization`.
	public let name: String?
	/// Header value. Include `Bearer ` before an API key or JWT.
	public let value: String
	/// Expiry in Unix seconds. The SDK refreshes an expired credential before a request.
	public let expiresAtSeconds: Int64

	public init(name: String? = nil, value: String, expiresAtSeconds: Int64) {
		self.name = name
		self.value = value
		self.expiresAtSeconds = expiresAtSeconds
	}

	/// Credential contents are omitted from diagnostic output.
	public var description: String {
		"Credential(<redacted>)"
	}

	public var debugDescription: String {
		description
	}

	func toFfi() -> FfiCredential {
		FfiCredential(name: name, value: value, expiresAtSeconds: expiresAtSeconds)
	}
}

/// Supplies a credential when the backend needs one, including after expiry or rejection.
/// The SDK caches the credential. Callback failures do not expose the app's error text.
public typealias AuthCallback = @Sendable () async throws -> Credential

final class FfiAuthCallbackAdapter: FfiAuthCallback {
	private let callback: AuthCallback

	init(_ callback: @escaping AuthCallback) {
		self.callback = callback
	}

	func onAuthRequired() async throws -> FfiCredential {
		do {
			return try await callback().toFfi()
		} catch is CancellationError {
			throw CancellationError()
		} catch {
			throw FfiAuthCallbackError.Failed
		}
	}
}
