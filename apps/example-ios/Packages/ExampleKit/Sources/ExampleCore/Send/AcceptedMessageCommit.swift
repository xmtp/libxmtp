import Foundation

@MainActor public final class AcceptedMessageCommit {
	public let messageID: String
	private let persist: () throws -> Void
	private let verify: () throws -> Bool
	private let acknowledge: () throws
		-> Void
	private var complete = false
	public init(
		messageID: String,
		persist: @escaping () throws -> Void,
		verify: @escaping () throws -> Bool,
		acknowledge: @escaping () throws -> Void,
	) {
		self.messageID = messageID
		self.persist = persist
		self.verify = verify
		self.acknowledge = acknowledge
	}

	public func finish() throws {
		guard !complete else { return }
		try persist()
		guard try verify()
		else { throw ChatError.storage }
		try acknowledge()
		complete = true
	}
}
