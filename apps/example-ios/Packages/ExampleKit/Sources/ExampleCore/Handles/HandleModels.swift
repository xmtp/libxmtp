import Foundation

public enum HandleMode: Equatable, Sendable { case choose, change }
public enum HandleProblem: String,
	Sendable { case invalidCharacters, startWithLetter, tooLong, tooShort, badCode, current }
public enum HandleRules {
	public static func normalize(_ value: String)
		-> String
	{
		String(String
			.UnicodeScalarView(value.unicodeScalars
				.map { (65 ... 90).contains($0.value) ? (UnicodeScalar($0.value + 32) ?? $0) : $0 }))
	}

	public static func name(_ value: String) -> String {
		String(value.prefix { $0 != "#" })
	}

	public static func problem(_ value: String, minimum: Int) -> HandleProblem? {
		if value.isEmpty {
			return nil
		}; let name = name(value); let code = value.dropFirst(name.count)
		if name.unicodeScalars
			.contains(where: { !(97 ... 122).contains($0.value) && !(48 ... 57).contains($0.value) && $0 != "_" && $0 != "-" })
		{
			return .invalidCharacters
		}
		guard let first = name.unicodeScalars.first, (97 ... 122).contains(first.value) else { return .startWithLetter }
		if name.count > 27 {
			return .tooLong
		}; if name.count < minimum {
			return .tooShort
		}
		if !code.isEmpty,
		   code.count != 5 || !code.dropFirst().utf8.allSatisfy({ (48 ... 57).contains($0) })
		{
			return .badCode
		}; return nil
	}
}

public struct OwnHandles: Equatable, Sendable {
	public var current: String?; public var others: [String]
	public init(current: String?, others: [String] = []) {
		self.current = current; self.others = others
	}
}

public enum HandleError: Error, Equatable,
	Sendable { case taken, notAllowed, tooSoon, limitReached, invalidArgument, failed }
@MainActor public protocol HandleAccount {
	var minimum: Int { get }; var claimTimes: [String: Date] { get }
	func handles() async throws -> OwnHandles
	func availableHandles(name: String) async throws -> [String]
	func claimHandle(handle: String) async throws
	func recordClaim(handle: String, at date: Date) throws
}

public enum HandleStatus: Equatable, Sendable {
	case editing, checking, available(String), suggestions(input: String, handles: [String], shown: Int, selected: String),
	     noneFree(String)
	public var pick: String? {
		switch self {
		case let .available(handle): handle
		case let .suggestions(_, _, _, handle): handle
		default: nil
		}
	}

	public var visible: [String] {
		if case let .suggestions(_, handles, shown, _) = self {
			return Array(handles.prefix(shown))
		}; return []
	}

	public var more: Bool {
		if case let .suggestions(_, handles, shown, _) = self {
			return shown < handles.count
		}; return false
	}
}

public enum HandleNotice: Equatable, Sendable {
	case taken(String)
	case notAllowed(String)
	case tooSoon(changed: Date?, next: Date?)
	case limitReached
	case checkFailed
	case claimFailed(String)
	case recordFailed(String)
	case switchBackBlocked(String, shorter: Bool)
}

public struct HandleState: Equatable, Sendable {
	public var mode: HandleMode; public var loading =
		true; public var current: String?; public var currentSince: Date?; public var others: [String] =
		[]; public var minimum = 3; public var input =
		""; public var problem: HandleProblem?; public var status: HandleStatus =
		.editing; public var claiming: String?; public var confirm: String?; public var notice: HandleNotice?
	public var blocked: Bool {
		switch notice {
		case .tooSoon, .limitReached: true
		default: false
		}
	}

	public var editable: Bool {
		if case .recordFailed = notice {
			return false
		}; return claiming == nil && !blocked
	}

	public var canCheck: Bool {
		editable && !loading && !input
			.isEmpty && problem == nil && status == .editing && notice != .notAllowed(HandleRules.name(input))
	}

	public var canClaim: Bool {
		editable && status.pick != nil
	}

	public init(mode: HandleMode) {
		self.mode = mode
	}
}

public enum HandleAction: Sendable { case load, changeInput(String), check, showMore, select(String), claim,
                                          switchBack(String), confirmClaim, cancelClaim, retry, skip
}
