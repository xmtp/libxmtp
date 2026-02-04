//
//  CodecRegistry.swift
//
//
//  Created by Pat Nakajima on 12/22/22.
//

import Foundation

class CodecRegistry {
	private let lock = NSLock()
	private var codecs: [String: any ContentCodec] = [
		TextCodec().id: TextCodec(),
	]

	func register(codec: any ContentCodec) {
		lock.lock()
		defer { lock.unlock() }
		codecs[codec.id] = codec
	}

	func find(for contentType: ContentTypeID?) -> any ContentCodec {
		lock.lock()
		defer { lock.unlock() }

		guard let contentType else {
			return TextCodec()
		}

		if let codec = codecs[contentType.id] {
			return codec
		}

		return TextCodec()
	}

	func find(for contentTypeString: String) -> any ContentCodec {
		lock.lock()
		defer { lock.unlock() }

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
		lock.lock()
		defer { lock.unlock() }
		return codecs[codec.id] != nil
	}

	func isRegistered(codecId: String) -> Bool {
		lock.lock()
		defer { lock.unlock() }
		return codecs[codecId] != nil
	}

	func removeCodec(for id: String) {
		lock.lock()
		defer { lock.unlock() }
		codecs.removeValue(forKey: id)
	}
}
