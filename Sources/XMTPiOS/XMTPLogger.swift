//
//  Logger.swift
//
//
//  Created by Pat Nakajima on 8/28/23.
//

import Foundation
import LibXMTP
import os

class XMTPLogger: FfiLogger {
	let logger = Logger()

	func log(level: UInt32, levelLabel: String, message: String) {
		switch level {
		case 1:
			logger.error("libxmtp[\(levelLabel)] - \(message)")
		case 2, 3:
			logger.info("libxmtp[\(levelLabel)] - \(message)")
		case 4:
			logger.debug("libxmtp[\(levelLabel)] - \(message)")
		case 5:
			logger.trace("libxmtp[\(levelLabel)] - \(message)")
		default:
			print("libxmtp[\(levelLabel)] - \(message)")
		}
	}
}
