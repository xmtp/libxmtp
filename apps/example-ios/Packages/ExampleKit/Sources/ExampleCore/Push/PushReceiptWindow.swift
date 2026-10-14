import Foundation

/// Bounded replay state. Backend sequence IDs are unique across topics (API-287).
/// Older evicted notifications stay stale; they never become eligible for replay.
public struct PushReceiptWindow: Codable, Sendable {
	private static let capacity = 512
	private var retiredThrough: UInt64?
	private var sequences: [UInt64] = []

	public init() {}

	public func contains(_ tag: String) -> Bool {
		guard let sequence = Self.sequence(tag) else { return false }
		return retiredThrough.map { sequence <= $0 } == true || sequences.contains(sequence)
	}

	public mutating func record(_ tag: String) {
		guard let sequence = Self.sequence(tag), !contains(tag) else { return }
		sequences.append(sequence)
		sequences.sort()
		if sequences.count > Self.capacity {
			retiredThrough = sequences.removeFirst()
		}
	}

	private static func sequence(_ tag: String) -> UInt64? {
		guard let separator = tag.lastIndex(of: ":") else { return nil }
		let topic = String(tag[..<separator])
		let value = String(tag[tag.index(after: separator)...])
		return PushEnvelope(topic: topic, sequence: value)?.sequence
	}
}
