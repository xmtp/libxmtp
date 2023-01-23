//
//  SendOptions.swift
//
//
//  Created by Pat Nakajima on 1/19/23.
//

import Foundation

public struct SendOptions {
	public var compression: EncodedContentCompression? = nil
	public var contentType: ContentTypeID?
	public var contentFallback: String?
}
