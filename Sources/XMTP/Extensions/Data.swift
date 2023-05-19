//
//  Data.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import XMTPRust

extension Data {
	init?(base64String: String) {
		self.init(base64Encoded: Data(base64String.utf8))
	}

	init(_ rustVec: RustVec<UInt8>) {
		self.init(rustVec.map { $0 })
	}

	var toHex: String {
		return reduce("") { $0 + String(format: "%02x", $1) }
	}
}
