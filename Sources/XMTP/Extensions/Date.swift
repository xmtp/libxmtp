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
}
