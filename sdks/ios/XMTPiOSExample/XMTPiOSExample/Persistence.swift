//
//  Persistence.swift
//
//
//  Created by Pat Nakajima on 1/20/23.
//

import Foundation
import KeychainAccess
import XmtpSdk

struct Persistence {
	var keychain: Keychain

	init() {
		keychain = Keychain(service: "com.xmtp.XMTPiOSExample")
	}

	func saveKeys(_ keys: Data) {
		keychain[data: "keys"] = keys
	}

	func loadKeys() -> Data? {
		do {
			return try keychain.getData("keys")
		} catch {
			print("Error loading keys data: \(error)")
			return nil
		}
	}

	func saveAddress(_ address: String) {
		keychain[string: "address"] = address
	}

	func loadAddress() -> String? {
		do {
			return try keychain.getString("address")
		} catch {
			print("Error loading address data: \(error)")
			return nil
		}
	}
}
