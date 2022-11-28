//
//  Util.swift
//
//
//  Created by Pat Nakajima on 11/20/22.
//

import Foundation
import web3

enum Util {
	static func keccak256(_ data: Data) -> Data {
		return data.web3.keccak256
	}
}
