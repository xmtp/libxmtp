import Foundation

public enum DisplayInitials {
	public static func group(_ title: String) -> String {
		let value = title.trimmingCharacters(in: .whitespacesAndNewlines)
		let words = value.drop(while: { $0 == "@" }).split(whereSeparator: { $0.isWhitespace })
		return words.prefix(2).compactMap(\.first).map(String.init).joined().uppercased()
	}

	public static func person(_ label: String, isInboxID: Bool = false) -> String {
		let value = label.trimmingCharacters(in: .whitespacesAndNewlines)
		if isInboxID {
			return String(value.prefix(2)).lowercased()
		}
		return String(value.drop(while: { $0 == "@" }).prefix(1)).uppercased()
	}
}
