import OSLog

extension Logger {
	static func forClass(_ cls: AnyClass) -> Logger {
		Logger(
			subsystem: "example.com",
			category: String(describing: cls)
		)
	}
}
