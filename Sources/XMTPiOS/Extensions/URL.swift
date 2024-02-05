//
//  URL.swift
//  
//
//  Created by Pat Nakajima on 2/1/24.
//

import Foundation

extension URL {
	static var documentsDirectory: URL {
		// swiftlint:disable no_optional_try
		guard let documentsDirectory = try? FileManager.default.url(
			for: .documentDirectory,
			in: .userDomainMask,
			appropriateFor: nil,
			create: false
		) else {
			fatalError("No documents directory")
		}

		return documentsDirectory
	}
}
