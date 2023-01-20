//
//  EncodedContentCompression.swift
//
//
//  Created by Pat Nakajima on 1/19/23.
//

import Foundation
import Gzip
import zlib

public enum EncodedContentCompression {
	case deflate, gzip

	func compress(content: Data) throws -> Data {
		switch self {
		case .deflate:
			// 78 9C - Default Compression according to https://www.ietf.org/rfc/rfc1950.txt
			let header = Data([0x78, 0x9C])

			// Perform rfc1951 compression
			let compressed = try (content as NSData).compressed(using: .zlib) as Data

			// Needed for rfc1950 compliance
			let checksum = adler32(content)

			return header + compressed + checksum
		case .gzip:
			return try content.gzipped()
		}
	}

	func decompress(content: Data) throws -> Data {
		switch self {
		case .deflate:
			// Swift uses https://www.ietf.org/rfc/rfc1951.txt while JS uses https://www.ietf.org/rfc/rfc1950.txt
			// They're basically the same except the JS version has a two byte header that we can just get rid of
			// and a four byte checksum at the end that seems to be ignored here.
			let data = NSData(data: content[2...])
			let inflated = try data.decompressed(using: .zlib)
			return inflated as Data
		case .gzip:
			return try content.gunzipped()
		}
	}

	private func adler32(_ data: Data) -> Data {
		let prime = UInt32(65521)
		var s1 = UInt32(1 & 0xFFFF)
		var s2 = UInt32((1 >> 16) & 0xFFFF)
		data.forEach {
			s1 += UInt32($0)
			if s1 >= prime { s1 = s1 % prime }
			s2 += s1
			if s2 >= prime { s2 = s2 % prime }
		}
		var result = ((s2 << 16) | s1).bigEndian
		return Data(bytes: &result, count: MemoryLayout<UInt32>.size)
	}
}
