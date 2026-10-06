import Foundation
import os

/// Send SDK records to the Apple system log. Persistent writers remain in Rust.
public final class AppleLogSink: LogSink, @unchecked Sendable {
	private let logger: Logger

	public init(subsystem: String = Bundle.main.bundleIdentifier ?? "org.xmtp", category: String = "XMTP") {
		logger = Logger(subsystem: subsystem, category: category)
	}

	public func log(record: LogRecord) async throws {
		let level: OSLogType
		switch record.level {
		case .off: return
		case .error: level = .error
		case .warn: level = .default
		case .info: level = .info
		case .debug, .trace: level = .debug
		}
		let fields = record.fields.sorted { $0.key < $1.key }.map { "\($0.key)=\($0.value)" }.joined(separator: " ")
		logger.log(
			level: level,
			"[\(record.target, privacy: .private)] \(record.message, privacy: .private) \(fields, privacy: .private) dropped=\(record.droppedRecords)",
		)
	}
}
