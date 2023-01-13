//
//  CodecRegistry.swift
//
//
//  Created by Pat Nakajima on 12/22/22.
//

import Foundation

struct CodecRegsistry {
	var codecs: [String: any ContentCodec] = [:]

	mutating func register(codec: any ContentCodec) {
		codecs[codec.id] = codec
	}

	func find(for contentType: ContentTypeID) -> (any ContentCodec)? {
		if let codec = codecs[contentType.id] {
			return codec
		}

		return nil
	}
}
