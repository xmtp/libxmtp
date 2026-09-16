import Foundation

/// The six server-configuration conditions of spec 006 CFG-083, surfaced as
/// distinct Swift types.
///
/// The uniffi binding declares `FfiError` as a flat error, so every one of
/// these arrives as its own case of the generated enum carrying only the
/// `Display` string. The SDK re-wraps each case in its own type, the way
/// ``NotificationError`` re-wraps a notification failure, so an app can write
/// `catch is BackendMismatchError` instead of matching on a message. Catch
/// ``ServerConfigurationError`` to handle all six at once.
///
/// The structured payloads — the two identifiers of a backend mismatch, the two
/// versions of a version rejection, the required scopes, the rejected chain and
/// the accepted list — do not cross the FFI boundary, because a flat error
/// publishes only its message. Read them from ``ServerConfiguration``, which
/// carries every one of them, and read ``message`` for the operator-facing text.
public protocol ServerConfigurationError: Error, CustomStringConvertible,
	LocalizedError
{
	/// The message the client produced, prefixed with its error code.
	var message: String { get }
}

public extension ServerConfigurationError {
	var description: String {
		message
	}

	var errorDescription: String? {
		message
	}
}

/// CFG-041: the backend did not serve its configuration, or the answer could
/// not be stored.
public struct ConfigurationUnavailableError: ServerConfigurationError, Equatable,
	Sendable
{
	public let message: String
}

/// CFG-044: the backend published a configuration this client cannot use.
public struct ConfigurationInvalidError: ServerConfigurationError, Equatable,
	Sendable
{
	public let message: String
}

/// CFG-051: this database is bound to one backend and a different one answered.
/// Read the bound identifier from ``ServerConfiguration/identifier``.
public struct BackendMismatchError: ServerConfigurationError, Equatable, Sendable {
	public let message: String
}

/// CFG-060 and CFG-061: the backend requires a newer libxmtp than this build.
/// Read the minimum from ``ServerConfiguration/minLibxmtpVersion`` and this
/// build's version from ``Client/libXMTPVersion``.
public struct ClientVersionTooOldError: ServerConfigurationError, Equatable,
	Sendable
{
	public let message: String
}

/// CFG-062: the backend requires a credential and none was configured. Read the
/// scopes from ``AuthConfiguration/requiredScopes``.
public struct AuthRequiredError: ServerConfigurationError, Equatable, Sendable {
	public let message: String
}

/// CFG-069 and CFG-070: the backend does not verify smart contract wallet
/// signatures on this chain. Read the accepted list from
/// ``ServerConfiguration/smartContractWalletChains``.
public struct ChainNotAcceptedError: ServerConfigurationError, Equatable, Sendable {
	public let message: String
}

public extension Error {
	/// The distinct server-configuration error this error carries, or `nil`
	/// when it is an ordinary failure.
	///
	/// Client creation, ``Client/serverConfiguration()``,
	/// ``Client/refreshServerConfiguration()`` and
	/// ``Client/fetchServerConfiguration(url:appVersion:)`` already throw the
	/// distinct type. This property covers every other call, which still throws
	/// the generated `FfiError`.
	var serverConfigurationError: (any ServerConfigurationError)? {
		if let alreadyMapped = self as? any ServerConfigurationError {
			return alreadyMapped
		}
		guard let error = self as? FfiError else { return nil }
		switch error {
		case .Error:
			return nil
		case let .ConfigurationUnavailable(message):
			return ConfigurationUnavailableError(message: message)
		case let .ConfigurationInvalid(message):
			return ConfigurationInvalidError(message: message)
		case let .BackendMismatch(message):
			return BackendMismatchError(message: message)
		case let .ClientVersionTooOld(message):
			return ClientVersionTooOldError(message: message)
		case let .AuthRequired(message):
			return AuthRequiredError(message: message)
		case let .ChainNotAccepted(message):
			return ChainNotAcceptedError(message: message)
		}
	}
}

/// Replace a generated configuration case with its distinct Swift type. Any
/// other error is returned unchanged, exactly as `NotificationError.from`.
func mapServerConfigurationError(_ error: Error) -> Error {
	error.serverConfigurationError ?? error
}
