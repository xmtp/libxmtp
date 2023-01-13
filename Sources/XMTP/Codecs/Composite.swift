//
//  CompositeCodec.swift
//
//
//  Created by Pat Nakajima on 12/22/22.
//

import XMTPProto

typealias Composite = Xmtp_MessageContents_Composite

let ContentTypeComposite = ContentTypeID(authorityID: "xmtp.org", typeID: "composite", versionMajor: 1, versionMinor: 0)

extension Composite.Part {
	init(encodedContent: EncodedContent) {
		self.init()
		element = .part(encodedContent)
	}

	init(composite: Composite) {
		self.init()
		element = .composite(composite)
	}
}

struct CompositeCodec: ContentCodec {
	public typealias T = DecodedComposite

	public var contentType: ContentTypeID {
		ContentTypeComposite
	}

	public func encode(content: DecodedComposite) throws -> EncodedContent {
		let composite = toComposite(content: content)
		var encoded = EncodedContent()
		encoded.type = ContentTypeComposite
		encoded.content = try composite.serializedData()
		return encoded
	}

	public func decode(content encoded: EncodedContent) throws -> DecodedComposite {
		let composite = try Composite(serializedData: encoded.content)
		let decodedComposite = fromComposite(composite: composite)
		return decodedComposite
	}

	func toComposite(content decodedComposite: DecodedComposite) -> Composite {
		var composite = Composite()

		if let content = decodedComposite.encodedContent {
			composite.parts = [Composite.Part(encodedContent: content)]
			return composite
		}

		for part in decodedComposite.parts {
			if let encodedContent = part.encodedContent {
				composite.parts.append(Composite.Part(encodedContent: encodedContent))
			} else {
				composite.parts.append(Composite.Part(composite: toComposite(content: part)))
			}
		}

		return composite
	}

	func fromComposite(composite: Composite) -> DecodedComposite {
		var decodedComposite = DecodedComposite()

		if composite.parts.count == 1, case let .part(content) = composite.parts.first?.element {
			decodedComposite.encodedContent = content
			return decodedComposite
		}

		decodedComposite.parts = composite.parts.map { fromCompositePart(part: $0) }

		return decodedComposite
	}

	func fromCompositePart(part: Composite.Part) -> DecodedComposite {
		var decodedComposite = DecodedComposite()

		switch part.element {
		case let .part(encodedContent):
			decodedComposite.encodedContent = encodedContent
		case let .composite(composite):
			decodedComposite.parts = composite.parts.map { fromCompositePart(part: $0) }
		case .none:
			return decodedComposite
		}

		return decodedComposite
	}
}
