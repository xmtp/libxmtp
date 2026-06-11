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
	public var ephemeral = false
	/// Optional idempotency key. Re-sending identical content with the same key
	/// produces the same message id and is deduplicated. Defaults to a timestamp.
	public var idempotencyKey: String?

	public init(compression: EncodedContentCompression? = nil, contentType: ContentTypeID? = nil,
	            ephemeral: Bool = false, idempotencyKey: String? = nil)
	{
		self.compression = compression
		self.contentType = contentType
		self.ephemeral = ephemeral
		self.idempotencyKey = idempotencyKey
	}
}
