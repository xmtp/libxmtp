import Foundation

/// Absence differs from an explicitly stored empty string.
public enum OwnFieldValue: Equatable, Sendable {
	case absent
	case value(String)
	public var text: String {
		if case let .value(text) = self {
			return text
		}
		return ""
	}
}

public struct OwnFieldDraft: Equatable, Sendable {
	private var baseline: [UInt16: OwnFieldValue] = [:]
	private var values: [UInt16: OwnFieldValue] = [:]
	public init() {}
	public func value(for id: UInt16) -> OwnFieldValue {
		values[id] ?? .absent
	}

	public func changed(_ id: UInt16) -> Bool {
		values[id] != baseline[id]
	}

	public mutating func set(_ id: UInt16, to value: OwnFieldValue) {
		values[id] = value
	}

	public mutating func merge(_ fields: [MetadataField]) {
		var nextBaseline: [UInt16: OwnFieldValue] = [:]
		var nextValues: [UInt16: OwnFieldValue] = [:]
		for field in fields where field.userField && field.canWrite {
			let committed: OwnFieldValue = field.present ? .value(field.scalar) : .absent
			if let old = baseline[field.id], let edited = values[field.id], edited != old, edited != committed {
				nextBaseline[field.id] = old
				nextValues[field.id] = edited
			} else {
				nextBaseline[field.id] = committed
				nextValues[field.id] = committed
			}
		}
		baseline = nextBaseline
		values = nextValues
	}

	public var changes: [UInt16: String?] {
		var result: [UInt16: String?] = [:]
		for (id, value) in values where value != baseline[id] {
			switch value {
			case .absent: result.updateValue(nil, forKey: id)
			case let .value(text): result.updateValue(text, forKey: id)
			}
		}
		return result
	}
}
