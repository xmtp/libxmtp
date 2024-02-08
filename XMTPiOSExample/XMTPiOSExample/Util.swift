//
//  Util.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 2/6/24.
//

import Foundation

enum Util {
	static func abbreviate(address: String) -> String {
		if address.count > 6 {
			let start = address.index(address.startIndex, offsetBy: 6)
			let end = address.index(address.endIndex, offsetBy: -5)
			return address.replacingCharacters(in: start ... end, with: "...")
		} else {
			return address
		}
	}
}
