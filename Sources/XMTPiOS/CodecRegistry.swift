//
//  CodecRegistry.swift
//
//
//  Created by Pat Nakajima on 12/22/22.
//

import Foundation

struct CodecRegistry {
	var codecs: [String: any ContentCodec] = [
		TextCodec().id: TextCodec(),
	]

	mutating func register(codec: any ContentCodec) {
		codecs[codec.id] = codec
	}

	func find(for contentType: ContentTypeID?) -> any ContentCodec {
		guard let contentType else {
			return TextCodec()
		}

		if let codec = codecs[contentType.id] {
			return codec
		}

		return TextCodec()
	}

	func find(for contentTypeString: String) -> any ContentCodec {
		for (_, codec) in codecs {
			if codec.description == contentTypeString {
				return codec
			}
		}

		return TextCodec()
	}
}

extension CodecRegistry {
    func isRegistered(codec: any ContentCodec) -> Bool {
        return codecs[codec.id] != nil
    }
    
    func isRegistered(codecId: String) -> Bool {
        return codecs[codecId] != nil
    }
}
