//
//  Util.swift
//
//
//  Created by Pat Nakajima on 11/20/22.
//

import Foundation
import CryptoSwift

enum Util {
	static func keccak256(_ data: Data) -> Data {
		return data.sha3(.keccak256)
	}
}

extension Array {
    func chunks(_ chunkSize: Int) -> [[Element]] {
        return stride(from: 0, to: self.count, by: chunkSize).map {
            Array(self[$0..<Swift.min($0 + chunkSize, self.count)])
        }
    }
}
