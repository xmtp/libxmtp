//
//  ContentCodec.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import XMTPProto

typealias EncodedContent = Xmtp_MessageContents_EncodedContent

protocol ContentCodec {
	associatedtype T

	var contentType: ContentTypeID { get }
	func encode(content: T) throws -> EncodedContent
	func decode(content: EncodedContent) throws -> T
}
