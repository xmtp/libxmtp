import Foundation

/// Keeps streaming disk work and scoped access off the UI actor.
public actor AttachmentFileCopier {
	public static let shared = AttachmentFileCopier()
	public enum CopyError: Error { case tooLarge }
	private let beforeWrite: @Sendable () -> Void
	public init() {
		beforeWrite = {}
	}

	init(beforeWrite: @escaping @Sendable () -> Void) {
		self.beforeWrite = beforeWrite
	}

	public func copy(_ source: URL, to target: URL, limit: UInt64 = .max) throws {
		try Task.checkCancellation()
		let accessing = source.startAccessingSecurityScopedResource()
		defer {
			if accessing {
				source.stopAccessingSecurityScopedResource()
			}
		}
		guard FileManager.default.createFile(atPath: target.path, contents: nil) else { throw CocoaError(.fileWriteUnknown) }
		do {
			let input = try FileHandle(forReadingFrom: source)
			defer { do { try input.close() } catch {} }
			let output = try FileHandle(forWritingTo: target)
			defer { do { try output.close() } catch {} }
			var count: UInt64 = 0
			while true {
				try Task.checkCancellation()
				let remaining = limit - min(limit, count)
				let capacity = remaining >= 65536 ? 65536 : Int(remaining) + 1
				guard let bytes = try input.read(upToCount: capacity), !bytes.isEmpty else { break }
				count += UInt64(bytes.count)
				guard count <= limit else { throw CopyError.tooLarge }
				beforeWrite()
				try Task.checkCancellation()
				try output.write(contentsOf: bytes)
			}
			try Task.checkCancellation()
		} catch {
			try FileManager.default.removeItem(at: target)
			throw error
		}
	}
}
