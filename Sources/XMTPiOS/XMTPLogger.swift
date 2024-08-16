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
	let logger = Logger(subsystem: "XMTP", category: "libxmtp")

	func log(level: UInt32, levelLabel: String, message: String) {
		switch level {
		case 1:
			logger.error("libxmtp[\(levelLabel, privacy: .public)] - \(message, privacy: .public)")
		case 2, 3:
			logger.info("libxmtp[\(levelLabel, privacy: .public)] - \(message, privacy: .public)")
		case 4:
			logger.debug("libxmtp[\(levelLabel, privacy: .public)] - \(message, privacy: .public)")
		case 5:
			logger.trace("libxmtp[\(levelLabel, privacy: .public)] - \(message, privacy: .public)")
		default:
			print("libxmtp[\(levelLabel)] - \(message)")
		}
	}
}
