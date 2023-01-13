//
//  ContentCodec.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import XMTPProto

enum CodecError: String, Error {
	case invalidContent, codecNotFound
}

public typealias EncodedContent = Xmtp_MessageContents_EncodedContent

extension EncodedContent {
	func decoded<T>() throws -> T {
		guard let codec = Client.codecRegistry.find(for: type) else {
			throw CodecError.codecNotFound
		}

		if let content = try codec.decode(content: self) as? T {
			return content
		}

		throw CodecError.invalidContent
	}
}

public protocol ContentCodec: Hashable, Equatable {
	associatedtype T

	var contentType: ContentTypeID { get }
	func encode(content: T) throws -> EncodedContent
	func decode(content: EncodedContent) throws -> T
}

public extension ContentCodec {
	static func == (lhs: Self, rhs: Self) -> Bool {
		return lhs.contentType.authorityID == rhs.contentType.authorityID && lhs.contentType.typeID == rhs.contentType.typeID
	}

	var id: String {
		contentType.id
	}

	func hash(into hasher: inout Hasher) {
		hasher.combine(id)
	}
}
