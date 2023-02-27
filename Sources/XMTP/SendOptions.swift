//
//  SendOptions.swift
//
//
//  Created by Pat Nakajima on 1/19/23.
//

import Foundation

public struct SendOptions {
	public var compression: EncodedContentCompression?
	public var contentType: ContentTypeID?
	public var contentFallback: String?

	public init(compression: EncodedContentCompression? = nil, contentType: ContentTypeID? = nil, contentFallback: String? = nil) {
		self.compression = compression
		self.contentType = contentType
		self.contentFallback = contentFallback
	}
}
