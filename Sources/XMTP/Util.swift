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
		return Data(SHA3(variant: .keccak256).calculate(for: data.bytes))
	}
}
