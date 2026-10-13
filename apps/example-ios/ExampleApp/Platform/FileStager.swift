import ExampleCore
import Foundation

/// Source metadata is advisory. The streaming copy enforces the server's real limit.
enum FileStager {
	static func stage(_ source: URL, directory: URL, limit: UInt64) throws -> URL {
		let accessing = source.startAccessingSecurityScopedResource()
		defer {
			if accessing {
				source.stopAccessingSecurityScopedResource()
			}
		}
		let staging = AttachmentStagingFiles(temporaryDirectory: directory)
		let target = try staging.newFile()
		guard FileManager.default.createFile(atPath: target.path, contents: nil) else {
			throw CocoaError(.fileWriteUnknown)
		}
		do {
			let input = try FileHandle(forReadingFrom: source)
			defer { do { try input.close() } catch {} }
			let output = try FileHandle(forWritingTo: target)
			defer { do { try output.close() } catch {} }
			var count: UInt64 = 0
			while true {
				let remaining = limit - min(limit, count)
				let capacity = remaining >= 65536 ? 65536 : Int(remaining) + 1
				guard let bytes = try input.read(upToCount: capacity), !bytes.isEmpty else { break }
				count += UInt64(bytes.count)
				if count > limit {
					throw StagingError.tooLarge
				}
				try output.write(contentsOf: bytes)
			}
			return target
		} catch {
			try staging.removeOwned(target)
			throw error
		}
	}

	static func stage(_ bytes: Data, directory: URL, limit: UInt64) throws -> URL {
		guard UInt64(bytes.count) <= limit else { throw StagingError.tooLarge }
		let staging = AttachmentStagingFiles(temporaryDirectory: directory)
		let target = try staging.newFile()
		do {
			try bytes.write(to: target)
			return target
		} catch {
			try staging.removeOwned(target)
			throw error
		}
	}

	enum StagingError: Error { case tooLarge }
}
