import Foundation

public struct AttachmentState: Equatable, Sendable {
	public var limit: UInt64?
	public var drafts: [FileDraft] = []
	public var cards: [String: FileCard] =
		[:]
	public var error: String?
	public var busy = false
}

public enum AttachmentAction: Sendable {
	case load
	case stage(URL, String)
	case send(UUID)
	case discard(UUID)
	case assign(UUID, String)
	case download(String, FileDescriptor)
	case restore(String, FileDescriptor, own: Bool)
}

/// Persist only in encrypted preferences. Views display FileCard, which carries no keys or URL.
public struct FileDescriptor: Codable, Equatable, Sendable {
	public var url: String
	public var digest: String
	public var secret: Data
	public var salt: Data
	public var nonce: Data
	public var scheme: String
	public var length: UInt32?
	public var filename: String?
	public init(
		url: String,
		digest: String,
		secret: Data,
		salt: Data,
		nonce: Data,
		scheme: String,
		length: UInt32?,
		filename: String?,
	) {
		self.url = url
		self.digest = digest
		self.secret = secret
		self.salt = salt
		self.nonce = nonce
		self
			.scheme = scheme
		self.length = length
		self.filename = filename
	}
}

public enum FilePhase: String, Codable,
	Sendable
{
	case available, waiting, uploading, complete, downloading, verified, failed, unknown, expired
}

public struct FileCard: Identifiable, Equatable, Sendable {
	public var id: String
	public var name: String
	public var length: UInt32?
	public var phase: FilePhase
	public var error: String?
	public var retryable: Bool
	public init(
		id: String,
		name: String,
		length: UInt32?,
		phase: FilePhase = .available,
		error: String? = nil,
		retryable: Bool = false,
	) {
		self.id = id
		self.name = name
		self.length = length
		self.phase = phase
		self.error = error
		self
			.retryable = retryable
	}
}

public struct FileDraft: Codable, Equatable, Identifiable, Sendable {
	public var id: UUID
	public var conversationID: String?
	public var descriptor: FileDescriptor
	public var phase: FilePhase
	public var acceptedID: String?
	public var retryable =
		true
	public init(
		id: UUID = UUID(),
		conversationID: String?,
		descriptor: FileDescriptor,
		phase: FilePhase = .waiting,
		acceptedID: String? = nil,
	) {
		self.id = id
		self.conversationID = conversationID
		self.descriptor = descriptor
		self.phase = phase
		self
			.acceptedID = acceptedID
	}

	public var card: FileCard {
		FileCard(
			id: id.uuidString,
			name: descriptor.filename ?? "File",
			length: descriptor.length,
			phase: phase,
		)
	}
}

public struct VerifiedFile: Equatable, Sendable {
	public var url: URL
	public var mimeType: String?
	public var filename: String
	public init(url: URL, mimeType: String?, filename: String) {
		self.url = url
		self.mimeType = mimeType
		self.filename = filename
	}
}

public enum AttachmentError: Error, Equatable, Sendable {
	case failure(message: String, retryable: Bool)
	public var message: String {
		switch self {
		case let .failure(message, _): message
		}
	}

	public var retryable: Bool {
		switch self {
		case let .failure(_, retryable): retryable
		}
	}
}

public protocol AttachmentGateway: Sendable {
	func capability() async throws -> UInt64?
	func create(file: URL, filename: String) async throws -> FileDescriptor
	func pending() async throws -> [FileDescriptor]
	func upload(_ file: FileDescriptor) async throws -> FilePhase
	func localFile(_ file: FileDescriptor) async throws -> VerifiedFile?
	func download(_ file: FileDescriptor) async throws -> VerifiedFile
	func discard(_ file: FileDescriptor) async throws
}

/// Holds file references for the destination. Decoded images have a separate memory budget.
@MainActor public final class VerifiedFileCache {
	private var entries: [String: VerifiedFile] = [:]
	public init() {}
	public func get(_ id: String) -> VerifiedFile? {
		entries[id]
	}

	public func put(_ id: String, file: VerifiedFile) {
		entries[id] = file
	}

	public func clear() {
		entries.removeAll()
	}
}

extension AttachmentAction {
	var downloadID: String? {
		switch self {
		case let .download(id, _), let .restore(id, _, _): id
		default: nil
		}
	}
}
