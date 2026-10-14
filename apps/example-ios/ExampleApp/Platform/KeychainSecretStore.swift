import ExampleCore
import Foundation
import Security

@MainActor final class KeychainSecretStore: SecretStore {
	private let service = "org.xmtp.example.ios"
	private func query(_ profile: UUID, _ ref: String) -> [String: Any] {
		[
			kSecClass as String: kSecClassGenericPassword,
			kSecAttrService as String: service,
			kSecAttrAccount as String: profile.uuidString + ":" + ref,
		]
	}

	func read(_ profile: UUID, ref: String) throws -> Data? {
		var query = query(profile,
		                  ref); query[kSecReturnData as String] = true; query[kSecMatchLimit as String] = kSecMatchLimitOne
		var result: CFTypeRef?; let status = SecItemCopyMatching(query as CFDictionary, &result)
		if status == errSecItemNotFound {
			return nil
		}; guard status == errSecSuccess,
		         let data = result as? Data
		else { throw keychainError(status) }; return data
	}

	func write(_ profile: UUID, ref: String, data: Data) throws {
		let query = query(profile, ref)
		let attributes: [String: Any] = [
			kSecValueData as String: data,
			kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
		]
		let update = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
		if update ==
			errSecItemNotFound
		{
			var item = query; attributes.forEach { item[$0.key] = $0.value }; let status = SecItemAdd(
				item as CFDictionary,
				nil,
			); guard status == errSecSuccess else { throw keychainError(status) }
		} else if update != errSecSuccess {
			throw keychainError(update)
		}
	}

	func remove(_ profile: UUID, ref: String) throws {
		let status = SecItemDelete(query(profile, ref) as CFDictionary); guard status == errSecSuccess || status ==
			errSecItemNotFound else { throw keychainError(status) }
	}

	func removeAll(_ profile: UUID) throws {
		for ref in ["wallet", "database-key", "credential", "preferences-key"] {
			try remove(profile, ref: ref)
		}
	}

	#if DEBUG
		func resetAllForTesting() throws {
			let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service]
			let status = SecItemDelete(query as CFDictionary)
			guard status == errSecSuccess || status == errSecItemNotFound else { throw keychainError(status) }
		}
	#endif

	func randomBytes(count: Int) throws -> Data {
		var data = Data(count: count); let status = data
			.withUnsafeMutableBytes { buffer in
				guard let pointer = buffer.baseAddress else { return errSecParam }; return SecRandomCopyBytes(
					kSecRandomDefault,
					count,
					pointer,
				)
			}; guard status == errSecSuccess else { throw keychainError(status) }; return data
	}

	private func keychainError(_ status: OSStatus) -> NSError {
		NSError(domain: NSOSStatusErrorDomain, code: Int(status))
	}
}
