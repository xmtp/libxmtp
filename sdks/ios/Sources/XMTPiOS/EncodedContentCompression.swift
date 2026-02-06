import Compression
import Foundation

public enum EncodedContentCompression {
	case deflate
	case gzip

	func compress(content: Data) -> Data? {
		switch self {
		case .deflate:
			compressData(content, using: COMPRESSION_ZLIB)
		case .gzip:
			compressData(content, using: COMPRESSION_LZFSE) // For GZIP, switch to COMPRESSION_ZLIB if needed.
		}
	}

	func decompress(content: Data) -> Data? {
		switch self {
		case .deflate:
			decompressData(content, using: COMPRESSION_ZLIB)
		case .gzip:
			decompressData(content, using: COMPRESSION_LZFSE) // For GZIP, switch to COMPRESSION_ZLIB if needed.
		}
	}

	/// Helper method to compress data using the Compression framework
	private func compressData(
		_ data: Data, using algorithm: compression_algorithm,
	) -> Data? {
		let destinationBuffer = UnsafeMutablePointer<UInt8>.allocate(
			capacity: data.count,
		)
		defer { destinationBuffer.deallocate() }

		let compressedSize = data.withUnsafeBytes { sourceBuffer -> Int in
			guard let sourcePointer = sourceBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
				return 0 // Return 0 to indicate failure
			}
			return compression_encode_buffer(
				destinationBuffer, data.count,
				sourcePointer, data.count, nil, algorithm,
			)
		}

		guard compressedSize > 0 else { return nil }
		return Data(bytes: destinationBuffer, count: compressedSize)
	}

	/// Helper method to decompress data using the Compression framework
	private func decompressData(
		_ data: Data, using algorithm: compression_algorithm,
	) -> Data? {
		let destinationBuffer = UnsafeMutablePointer<UInt8>.allocate(
			capacity: data.count * 4, // Allocate enough memory for decompressed data
		)
		defer { destinationBuffer.deallocate() }

		let decompressedSize = data.withUnsafeBytes { sourceBuffer -> Int in
			guard let sourcePointer = sourceBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
				return 0 // Return 0 to indicate failure
			}
			return compression_decode_buffer(
				destinationBuffer, data.count * 4,
				sourcePointer, data.count, nil, algorithm,
			)
		}

		guard decompressedSize > 0 else { return nil }
		return Data(bytes: destinationBuffer, count: decompressedSize)
	}
}
