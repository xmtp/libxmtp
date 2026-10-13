import Foundation

/// Owns only direct UUID-named files inside the profile's dedicated staging directory.
public struct AttachmentStagingFiles: Sendable {
	public let directory: URL
	private let temporaryDirectory: URL
	public init(temporaryDirectory: URL) {
		self.temporaryDirectory = temporaryDirectory.standardizedFileURL
		directory = self.temporaryDirectory.appendingPathComponent("attachment-staging", isDirectory: true)
	}

	public func newFile() throws -> URL {
		try prepare()
		return directory.appendingPathComponent(UUID().uuidString)
	}

	@discardableResult public func removeOwned(_ file: URL) throws -> Bool {
		let file = file.standardizedFileURL
		guard file.deletingLastPathComponent().path == directory.path, UUID(uuidString: file.lastPathComponent) != nil else {
			return false
		}
		try validateDirectories()
		guard let type = try itemType(file), type == .typeRegular || type == .typeSymbolicLink else { return false }
		try FileManager.default.removeItem(at: file)
		return true
	}

	public func removeAbandoned() throws {
		try prepare()
		for file in try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) {
			try removeOwned(file)
		}
	}

	private func prepare() throws {
		try validateDirectories()
		try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
	}

	private func validateDirectories() throws {
		for parent in [temporaryDirectory.deletingLastPathComponent(), temporaryDirectory, directory] {
			if let type = try itemType(parent), type != .typeDirectory {
				throw CocoaError(.fileWriteInvalidFileName)
			}
		}
	}

	private func itemType(_ url: URL) throws -> FileAttributeType? {
		do {
			return try FileManager.default.attributesOfItem(atPath: url.path)[.type] as? FileAttributeType
		} catch let error as CocoaError where error.code == .fileReadNoSuchFile {
			return nil
		}
	}
}
