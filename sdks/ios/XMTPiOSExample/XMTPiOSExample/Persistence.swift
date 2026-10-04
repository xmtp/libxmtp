//
//  Persistence.swift
//
//
//  Created by Pat Nakajima on 1/20/23.
//

import Foundation
import KeychainAccess

struct Persistence {
	struct Account: Codable {
		let databaseKey: Data
		let address: String
	}

	enum PersistenceError: Error {
		case invalidAccount
		case incompleteAccount
	}

	private let keychain: Keychain

	init() {
		keychain = Keychain(service: "com.xmtp.XMTPiOSExample")
	}

	func saveAccount(_ account: Account) throws {
		let account = try validated(account)
		try keychain.set(JSONEncoder().encode(account), key: "account-v1")
	}

	func loadAccount() throws -> Account? {
		if let data = try keychain.getData("account-v1") {
			return try validated(JSONDecoder().decode(Account.self, from: data))
		}

		let key = try keychain.getData("keys")
		let address = try keychain.getString("address")
		switch (key, address) {
		case (nil, nil):
			return nil
		case let (key?, address?):
			return try validated(Account(databaseKey: key, address: address))
		default:
			throw PersistenceError.incompleteAccount
		}
	}

	private func validated(_ account: Account) throws -> Account {
		guard account.databaseKey.count == 32, !account.address.isEmpty else {
			throw PersistenceError.invalidAccount
		}
		return account
	}
}
