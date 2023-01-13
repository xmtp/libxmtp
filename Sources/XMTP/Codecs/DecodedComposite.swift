//
//  DecodedComposite.swift
//
//
//  Created by Pat Nakajima on 12/22/22.
//

import Foundation

public struct DecodedComposite {
	var parts: [DecodedComposite] = []
	var encodedContent: EncodedContent?

	init(parts: [DecodedComposite] = [], encodedContent: EncodedContent? = nil) {
		self.parts = parts
		self.encodedContent = encodedContent
	}

	func content<T>() throws -> T? {
		return try encodedContent?.decoded()
	}
}
