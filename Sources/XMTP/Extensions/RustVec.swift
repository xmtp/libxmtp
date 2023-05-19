//
//  File.swift
//
//
//  Created by Pat Nakajima on 4/24/23.
//

import Foundation
import XMTPRust

extension RustVec where T == UInt8 {
	convenience init(_ data: Data) {
		self.init()

		for byte in data {
			push(value: byte)
		}
	}
}
