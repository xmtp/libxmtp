//
//  Data.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation

extension Data {
	var toHex: String {
		return reduce("") { $0 + String(format: "%02x", $1) }
	}
}
