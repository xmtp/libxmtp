import Foundation
import Security
import XmtpSdk

struct ExampleCredentials: Codable {
	let signerKey: Data
	let databaseKey: Data

	static func loadOrCreate(service: String) async throws -> ExampleCredentials {
		if let saved = try load(service: service) {
			return saved
		}
		let credentials = try ExampleCredentials(signerKey: randomKey(), databaseKey: randomKey())
		// Validate the signer before storing either key.
		_ = try await localSignerFromPrivateKey(key: credentials.signerKey)
		var query = keychainQuery(service: service)
		query[kSecValueData as String] = try JSONEncoder().encode(credentials)
		query[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
		let status = SecItemAdd(query as CFDictionary, nil)
		if status == errSecDuplicateItem, let saved = try load(service: service) {
			return saved
		}
		guard status == errSecSuccess else { throw keychainError(status) }
		return credentials
	}

	private static func load(service: String) throws -> ExampleCredentials? {
		var query = keychainQuery(service: service)
		query[kSecReturnData as String] = true
		query[kSecMatchLimit as String] = kSecMatchLimitOne
		var result: CFTypeRef?
		let status = SecItemCopyMatching(query as CFDictionary, &result)
		if status == errSecItemNotFound {
			return nil
		}
		guard status == errSecSuccess else { throw keychainError(status) }
		guard let data = result as? Data else { throw keychainError(errSecDecode) }
		let credentials = try JSONDecoder().decode(ExampleCredentials.self, from: data)
		guard credentials.signerKey.count == 32, credentials.databaseKey.count == 32 else {
			throw keychainError(errSecDecode)
		}
		return credentials
	}

	private static func keychainQuery(service: String) -> [String: Any] {
		[
			kSecClass as String: kSecClassGenericPassword,
			kSecAttrService as String: service,
			kSecAttrAccount as String: "xmtp-example-credentials-v1",
		]
	}

	private static func randomKey() throws -> Data {
		var bytes = [UInt8](repeating: 0, count: 32)
		let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
		guard status == errSecSuccess else { throw keychainError(status) }
		return Data(bytes)
	}

	private static func keychainError(_ status: OSStatus) -> NSError {
		NSError(domain: NSOSStatusErrorDomain, code: Int(status))
	}
}
