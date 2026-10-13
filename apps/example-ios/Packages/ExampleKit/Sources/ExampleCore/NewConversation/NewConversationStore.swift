import Foundation
import Observation

public struct Recipient: Equatable, Sendable, Identifiable {
	public var id: String {
		inboxID
	}

	public let inboxID: String
	public let handle: String?
	public init(inboxID: String, handle: String?) {
		self.inboxID = inboxID; self.handle = handle
	}

	public var label: String {
		handle.map { "@" + $0 } ?? String(inboxID.prefix(12))
	}
}

@MainActor public protocol NewConversationQueries {
	func people() async throws -> [Recipient]
	func maximumMembers() async throws -> Int
	func resolve(_ input: String) async throws -> Recipient
	func create(recipients: [String], group: Bool, name: String, description: String, adminOnly: Bool) async throws
		-> String
}

public struct NewConversationState: Equatable, Sendable {
	public var group = false
	public var input = ""
	public var recipients: [Recipient] = []
	public var people: [Recipient] = []
	public var name = ""
	public var description = ""
	public var adminOnly = false
	public var reviewing = false
	public var busy = false
	public var loadingPeople = false
	public var error: String?
	public var maximumMembers = 0
	public init() {}
	public var canContinue: Bool {
		!busy && !loadingPeople && (!recipients.isEmpty || !input.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
	}
}

public enum NewConversationAction: Sendable {
	case load, group(Bool), input(String), add, remove(String), select(Recipient), next, back, name(String),
	     description(String), adminOnly(Bool), create
}

@MainActor @Observable public final class NewConversationStore: FeatureStore {
	public private(set) var state = NewConversationState()
	private let queries: any NewConversationQueries
	private let origin: DestinationOrigin
	private let admit: @MainActor (DestinationOrigin) -> Bool
	private let finish: @MainActor (String) -> Void
	private var task: Task<Void, Never>?
	private var tasks: [UUID: Task<Void, Never>] = [:]
	private var revision = 0
	private var loadTask: Task<Void, Never>?
	private var loadRevision = 0
	public init(
		queries: any NewConversationQueries,
		origin: DestinationOrigin,
		admit: @escaping @MainActor (DestinationOrigin) -> Bool,
		finish: @escaping @MainActor (String) -> Void,
	) {
		self.queries = queries; self.origin = origin; self.admit = admit; self.finish = finish
	}

	private func apply(_ revision: Int, _ body: (inout NewConversationState) -> Void) {
		guard revision == self.revision, admit(origin) else { return }; body(&state)
	}

	public func send(_ action: NewConversationAction) {
		switch action {
		case .load: loadPeople()
		case .add, .next, .create: resolveAndContinue(action)
		default: edit(action)
		}
	}

	private func edit(_ action: NewConversationAction) {
		switch action {
		case let .group(value): cancel(); state.group = value; state.reviewing = false
		case let .input(value):
			guard state.input != value else { return }
			cancel(); state.input = value
		case let .remove(id): cancel(); state.recipients.removeAll { $0.id == id }
		case let .select(person): toggle(person)
		case .back: cancel(); state.reviewing = false
		default: editReview(action)
		}
	}

	private func editReview(_ action: NewConversationAction) {
		switch action {
		case let .name(value): state.name = value
		case let .description(value): state.description = value
		case let .adminOnly(value): state.adminOnly = value
		default: break
		}
	}

	private func toggle(_ person: Recipient) {
		cancel()
		if state.recipients.contains(where: { $0.id == person.id }) {
			state.recipients.removeAll { $0.id == person.id }
		} else {
			state.recipients.append(person)
		}
	}

	private func loadPeople() {
		loadTask?.cancel(); loadRevision += 1
		let expected = loadRevision
		state.loadingPeople = true
		let id = UUID()
		loadTask = Task {
			defer { self.tasks.removeValue(forKey: id) }
			do {
				let people = try await self.queries.people()
				let maximum = try await self.queries.maximumMembers()
				self.applyLoad(expected) {
					$0.people = people; $0.maximumMembers = maximum; $0.loadingPeople = false
				}
			} catch is CancellationError {} catch {
				self.applyLoad(expected) { $0.loadingPeople = false; $0.error = "Cannot load people. Try again." }
			}
		}
		tasks[id] = loadTask
	}

	private func resolveAndContinue(_ action: NewConversationAction) {
		let input = state.input
		let selected = state.recipients
		let proceed = if case .next = action {
			true
		} else {
			false
		}
		let create = if case .create = action {
			true
		} else {
			false
		}
		perform { revision in
			let resolved = try await self.resolve(input, selected: selected)
			guard self.admit(self.origin), revision == self.revision else { return }
			try self.validate(resolved)
			self.apply(revision) {
				$0.recipients = resolved; $0.input = ""
				if proceed {
					$0.reviewing = true
				}
			}
			if create {
				try await self.create(resolved, revision: revision)
			}
		}
	}

	private func resolve(_ input: String, selected: [Recipient]) async throws -> [Recipient] {
		var result = selected
		for entry in input.split(separator: ",", omittingEmptySubsequences: true) {
			let person = try await queries.resolve(String(entry))
			if !result.contains(where: { $0.id == person.id }) {
				result.append(person)
			}
		}
		return result
	}

	private func validate(_ recipients: [Recipient]) throws {
		guard !recipients.isEmpty else { throw NewConversationError.input("Add at least one recipient.") }
		guard state.group || recipients.count == 1 else {
			throw NewConversationError.input("A direct message needs one recipient.")
		}
		guard !state.group || recipients.count + 1 <= state.maximumMembers else {
			throw NewConversationError.input("This group has too many members.")
		}
	}

	private func create(_ recipients: [Recipient], revision: Int) async throws {
		let value = state
		let id = try await queries.create(
			recipients: recipients.map(\.inboxID), group: value.group, name: value.name,
			description: value.description, adminOnly: value.adminOnly,
		)
		apply(revision) { $0.busy = false }
		if admit(origin), revision == self.revision {
			finish(id)
		}
	}

	func waitForBootstrap() async {
		await loadTask?.value
	}

	private func applyLoad(_ expected: Int, _ change: (inout NewConversationState) -> Void) {
		guard expected == loadRevision, admit(origin), !Task.isCancelled else { return }; change(&state)
	}

	public func close() async {
		let pending = Array(tasks.values); pending.forEach { $0.cancel() }; for value in pending {
			await value.value
		}; tasks.removeAll()
	}

	private func cancel() {
		task?.cancel(); revision += 1; state.busy = false; state.error = nil
	}

	private func perform(_ operation: @escaping @MainActor (Int) async throws -> Void) {
		cancel(); state.busy = true; let current = revision
		let id = UUID()
		task = Task {
			defer { self.tasks.removeValue(forKey: id) }
			do { try await operation(current); self.apply(current) { $0.busy = false } } catch is CancellationError {} catch {
				self.apply(current) {
					$0.busy = false
					$0.error = (error as? NewConversationError)?.message ?? "Cannot complete this request. Try again."
				}
			}
		}
		tasks[id] = task
	}
}

public enum NewConversationError: Error {
	case input(String)
	public var message: String {
		switch self {
		case let .input(value): value
		}
	}
}
