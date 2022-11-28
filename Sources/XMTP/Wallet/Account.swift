//
//  Account.swift
//
//
//  Created by Pat Nakajima on 11/22/22.
//

import Foundation
import UIKit

public struct Account {
	public var connection: WalletConnection
	var isConnected: Bool = false

	public static func create() throws -> Account {
		let connection = WCWalletConnection()
		return try Account(connection: connection)
	}

	init(connection: WalletConnection) throws {
		self.connection = connection
	}

	public var address: String {
		connection.walletAddress ?? ""
	}

	public func preferredConnectionMethod() throws -> WalletConnectionMethodType {
		try connection.preferredConnectionMethod()
	}

	public func connect() async throws {
		try await connection.connect()
	}
}

extension Account: SigningKey {
	func sign(_ data: Data) async throws -> Signature {
		let signatureData = try await connection.sign(data)

		var signature = Signature()

		signature.ecdsaCompact.bytes = signatureData[0 ..< 64]
		signature.ecdsaCompact.recovery = UInt32(signatureData[64])

		return signature
	}

	func sign(message: String) async throws -> Signature {
		return try await sign(Data(message.utf8))
	}
}
