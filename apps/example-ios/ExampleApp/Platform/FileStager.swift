import ExampleCore
import Foundation

/// Source metadata is advisory. The streaming copy enforces the server's real limit.
enum FileStager {
	static func stage(_ source: URL, directory: URL, limit: UInt64) async throws -> URL {
		let staging = AttachmentStagingFiles(temporaryDirectory: directory)
		let target = try staging.newFile()
		try await AttachmentFileCopier.shared.copy(source, to: target, limit: limit)
		return target
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

	typealias StagingError = AttachmentFileCopier.CopyError
}
