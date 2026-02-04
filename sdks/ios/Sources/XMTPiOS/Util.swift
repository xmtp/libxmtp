//
//  Util.swift
//
//
//  Created by Pat Nakajima on 11/20/22.
//

import CryptoSwift
import Foundation

enum Util {
	static func keccak256(_ data: Data) -> Data {
		data.sha3(.keccak256)
	}
}

extension Array {
	func chunks(_ chunkSize: Int) -> [[Element]] {
		stride(from: 0, to: count, by: chunkSize).map {
			Array(self[$0 ..< Swift.min($0 + chunkSize, self.count)])
		}
	}
}

func validateInboxId(_ inboxId: String) throws {
	if inboxId.lowercased().hasPrefix("0x") {
		throw ClientError.invalidInboxId(inboxId)
	}
}

func validateInboxIds(_ inboxIds: [String]) throws {
	for inboxId in inboxIds {
		try validateInboxId(inboxId)
	}
}
