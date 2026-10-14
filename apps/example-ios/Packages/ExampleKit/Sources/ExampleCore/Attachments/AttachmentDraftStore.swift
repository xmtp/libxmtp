import Foundation

@MainActor public final class AttachmentDraftStore {
	private let names: () throws -> [String]
	private let read: (String) throws -> FileDraft?
	private let write: (String, FileDraft) throws -> Void
	private let remove: (String) throws -> Void
	private let references: () throws -> [String]
	private let saveReferences: ([String]) throws -> Void
	public init(
		names: @escaping () throws -> [String],
		read: @escaping (String) throws -> FileDraft?,
		write: @escaping (String, FileDraft) throws -> Void,
		remove: @escaping (String) throws -> Void,
		references: @escaping () throws -> [String],
		saveReferences: @escaping ([String]) throws -> Void,
	) {
		self.names = names
		self.read = read
		self.write = write
		self.remove = remove
		self.references = references
		self
			.saveReferences = saveReferences
	}

	public func all() throws -> [FileDraft] {
		try references().compactMap { try read($0) }
	}

	public func save(_ draft: FileDraft) throws {
		let key = "attachment-" + draft.id.uuidString
		try write(key, draft)
		var refs = try references()
		if !refs.contains(key) {
			refs.append(key)
			try saveReferences(refs)
		}
	}

	public func remove(_ draft: FileDraft) throws {
		let key = "attachment-" + draft.id.uuidString
		try remove(key)
		try saveReferences(references().filter { $0 != key })
	}

	public func cleanOrphans() throws {
		let refs = try Set(references())
		for key in try names() where !refs.contains(key) {
			try remove(key)
		}
		let surviving = try references().filter { try read($0) != nil }
		try saveReferences(surviving)
	}
}
