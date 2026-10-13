import Foundation

/// A cancelled transfer may finish its importer later; late registrations are deleted immediately.
public actor AttachmentImportFiles {
	private let owner: AttachmentStagingFiles
	private var files: [URL] = []
	private var closed = false
	public init(owner: AttachmentStagingFiles) {
		self.owner = owner
	}

	public func record(_ file: URL) throws {
		if closed {
			do { try owner.removeOwned(file) } catch { files.append(file); throw error }
			throw CancellationError()
		}
		files.append(file)
	}

	public func close() throws {
		closed = true
		let pending = files
		var failed: [URL] = []
		var firstError: (any Error)?
		for file in pending {
			do { try owner.removeOwned(file) } catch { failed.append(file); firstError = firstError ?? error }
		}
		files = failed
		if let firstError {
			throw firstError
		}
	}
}
