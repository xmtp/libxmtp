//
//  Date.swift
//
//
//  Created by Pat Nakajima on 11/18/22.
//

import Foundation

extension Date {
	var millisecondsSinceEpoch: Double {
		timeIntervalSince1970 * 1000
	}

	init(millisecondsSinceEpoch: Int64) {
		self.init(timeIntervalSince1970: TimeInterval(millisecondsSinceEpoch / 1_000_000_000))
	}
}
