import Foundation

public enum HandleCopy {
	public static func problem(_ problem: HandleProblem, input: String, minimum: Int) -> String {
		switch problem {
		case .invalidCharacters: "Use only lowercase letters, numbers, _ and -."
		case .startWithLetter: "Start with a letter."
		case .tooLong: "Use 27 characters or fewer before #."
		case .tooShort: "Use at least \(minimum) characters on this server."
		case .badCode: "After #, use exactly 4 digits, like \(HandleRules.name(input))#0012."
		case .current: "This is your current handle."
		}
	}

	public static func notice(_ notice: HandleNotice) -> String {
		switch notice {
		case let .taken(handle): "Someone claimed \(handle) a moment before you. Pick another handle."
		case let .notAllowed(handle): "\(handle) is not allowed on this server."
		case let .tooSoon(changed, next): tooSoon(changed: changed, next: next)
		case .limitReached:
			"This inbox has used all 64 handle changes. "
				+ "Your current handle stays, and so do your other handles."
		case .checkFailed: "Cannot check this handle. Try again."
		case let .recordFailed(handle):
			"Your handle is \(handle), but its change date could not be saved. "
				+ "Retry to save it on this device."
		case let .claimFailed(handle): "Cannot claim \(handle). Try again."
		case let .switchBackBlocked(handle, shorter): switchBackBlocked(handle: handle, shorter: shorter)
		}
	}

	private static func tooSoon(changed: Date?, next: Date?) -> String {
		guard let changed, let next else {
			return "You changed your handle less than 7 days ago. Try again later."
		}
		let changedDay = changed.formatted(date: .abbreviated, time: .omitted)
		let nextDay = next.formatted(date: .abbreviated, time: .omitted)
		return "You changed your handle on \(changedDay). You can change it again on \(nextDay)."
	}

	private static func switchBackBlocked(handle: String, shorter: Bool) -> String {
		let reason = shorter
			? "\(handle) is shorter than this server now allows. "
			: "\(handle) is not allowed on this server now. "
		return reason + "It still finds you, but you cannot switch back to it."
	}
}
