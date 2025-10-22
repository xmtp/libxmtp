import Foundation

public struct SignedData {
	/// The raw signature bytes.
	public let rawData: Data

	/// The public key used for signing (required for Passkeys).
	public let publicKey: Data?

	/// The WebAuthn authenticator data (only for Passkeys).
	public let authenticatorData: Data?

	/// The WebAuthn client data JSON (only for Passkeys).
	public let clientDataJson: Data?

	public init(rawData: Data, publicKey: Data? = nil, authenticatorData: Data? = nil, clientDataJson: Data? = nil) {
		self.rawData = rawData
		self.publicKey = publicKey
		self.authenticatorData = authenticatorData
		self.clientDataJson = clientDataJson
	}
}
