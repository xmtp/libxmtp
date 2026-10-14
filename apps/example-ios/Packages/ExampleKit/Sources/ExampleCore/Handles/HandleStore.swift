import Foundation
import Observation

@MainActor @Observable public final class HandleStore {
	public private(set) var state: HandleState
	private let account: any HandleAccount
	private let origin: DestinationOrigin
	private let admit: @MainActor (DestinationOrigin) -> Bool
	private let sessionValid: @MainActor (SessionKey) -> Bool
	private let finish: @MainActor () -> Void
	private let persistSkip: (@MainActor () throws -> Void)?
	private var work: Task<Void, Never>?
	private var load: Task<Void, Never>?
	private var loadAttempt = 0
	private var tasks: [UUID: Task<Void, Never>] = [:]
	private var attempt = 0
	private var closed = false
	private var pendingRecord: (handle: String, at: Date)?
	public init(
		account: any HandleAccount,
		mode: HandleMode,
		origin: DestinationOrigin,
		admit: @escaping @MainActor (DestinationOrigin) -> Bool,
		finish: @escaping @MainActor () -> Void,
		sessionValid: (@MainActor (SessionKey) -> Bool)? = nil,
		persistSkip: (@MainActor () throws -> Void)? = nil,
	) {
		self.persistSkip = persistSkip
		self.sessionValid = sessionValid ?? { _ in admit(origin) }
		self.account = account; self.origin = origin; self.admit = admit; self.finish = finish
		state = HandleState(mode: mode); state.minimum = (1 ... 27).contains(account.minimum) ? account.minimum : 3
	}

	private func apply(_ number: Int, _ body: (inout HandleState) -> Void) -> Bool {
		guard !closed, number == attempt, admit(origin), !Task.isCancelled else { return false }; body(&state); return true
	}

	public func send(_ action: HandleAction) {
		guard !closed else { return }
		if handleEditingAction(action) {
			return
		}
		handleClaimAction(action)
	}

	private func handleEditingAction(_ action: HandleAction) -> Bool {
		switch action {
		case .load: loadAccount()
		case let .changeInput(value): edit(value)
		case .check: check()
		case .showMore: showMore()
		case let .select(handle): select(handle)
		default: return false
		}
		return true
	}

	private func handleClaimAction(_ action: HandleAction) {
		switch action {
		case .claim: claimSelection()
		case let .switchBack(handle):
			if state.editable, state.mode == .change, state.others.contains(handle) {
				state.confirm = handle
			}
		case .confirmClaim:
			if let handle = state.confirm {
				claim(handle)
			}
		case .cancelClaim: state.confirm = nil
		case .retry: retry()
		case .skip: skip()
		default: break
		}
	}

	private func loadAccount() {
		launchLoad { number in
			do {
				let own = try await self.account.handles()
				let applied = self.applyLoad(number) {
					$0.loading = false
					$0.current = own.current
					$0.others = own.others
					$0.currentSince = own.current.flatMap { self.account.claimTimes[$0] }
				}
				if applied, own.current != nil, self.pendingRecord == nil, self.state.mode == .choose {
					self.finish()
				}
			} catch { _ = self.applyLoad(number) { $0.loading = false } }
		}
	}

	private func edit(_ value: String) {
		guard state.editable else { return }
		let normalized = HandleRules.normalize(value)
		guard normalized != state.input else { return }
		cancel()
		state.input = normalized
		state.problem = HandleRules.problem(state.input, minimum: state.minimum)
		state.status = .editing
		state.confirm = nil
		state.notice = nil
	}

	private func check() {
		guard state.editable, !state.input.isEmpty, state.problem == nil else { return }
		if state.input == state.current {
			state.problem = .current; return
		}
		if state.mode == .change, state.others.contains(state.input) {
			state.confirm = state.input; return
		}
		let input = state.input
		state.status = .checking
		state.notice = nil
		launch { await self.answer($0, input: input) }
	}

	private func claimSelection() {
		guard state.canClaim, let handle = state.status.pick else { return }
		if state.mode == .change {
			state.confirm = handle
		} else {
			claim(handle)
		}
	}

	private func retry() {
		switch state.notice {
		case .recordFailed: persistRecord()
		case .skipFailed: skip()
		case let .claimFailed(handle): claim(handle)
		case .checkFailed: check()
		default: break
		}
	}

	private func applyLoad(_ number: Int, _ body: (inout HandleState) -> Void) -> Bool {
		guard !closed, number == loadAttempt, admit(origin), !Task.isCancelled else { return false }
		body(&state); return true
	}

	/// Reading the current account survives edits and availability checks.
	private func launchLoad(_ operation: @escaping @MainActor (Int) async -> Void) {
		load?.cancel(); loadAttempt += 1
		let number = loadAttempt; let id = UUID()
		let task = Task { await operation(number); self.tasks[id] = nil }
		tasks[id] = task; load = task
	}

	private func cancel() {
		work?.cancel(); attempt += 1
	}

	public func close() async {
		closed = true
		cancel()
		let pending = Array(tasks.values)
		for task in pending {
			task.cancel()
		}
		for task in pending {
			await task.value
		}
		tasks.removeAll()
	}

	private func launch(_ operation: @escaping @MainActor (Int) async -> Void) {
		cancel(); let number = attempt; let id = UUID()
		let task = Task { await operation(number); self.tasks[id] = nil }
		tasks[id] = task; work = task
	}

	private func answer(_ number: Int, input: String, except: String? = nil, notice: HandleNotice? = nil) async {
		do {
			let free = try await account.availableHandles(name: input).filter { $0 != except }
			let status: HandleStatus
			if free.contains(input) {
				status = .available(input)
			} else {
				let name = HandleRules.name(input)
				let alternatives = name == input ? free : try await account.availableHandles(name: name)
				let handles = alternatives.filter { $0 != input && $0 != except }
				if let first = handles.first {
					status = .suggestions(
						input: input,
						handles: handles,
						shown: min(3, handles.count),
						selected: first,
					)
				} else {
					status = .noneFree(input)
				}
			}
			_ = apply(number) { $0.claiming = nil; $0.status = status; $0.notice = notice }
		} catch {
			_ = apply(number) {
				$0.claiming = nil; $0
					.status = .editing; if error as? HandleError == .invalidArgument
				{
					$0.problem = .invalidCharacters
				} else {
					$0.notice = .checkFailed
				}
			}
		}
	}

	private func persistRecord() {
		guard !closed, admit(origin), let pendingRecord else { return }
		do {
			try account.recordClaim(handle: pendingRecord.handle, at: pendingRecord.at)
			self.pendingRecord = nil
			if apply(attempt, { $0.notice = nil }) {
				finish()
			}
		} catch { _ = apply(attempt) { $0.notice = .recordFailed(pendingRecord.handle) } }
	}

	private func claim(_ handle: String) {
		guard state.editable
		else { return }; let previous = state; state.claiming = handle; state.confirm = nil; state.notice = nil
		launch { number in
			do {
				try await self.account.claimHandle(handle: handle)
				guard self.sessionValid(self.origin.session) else { return }
				let now = Date()
				guard !self.closed, self.admit(self.origin), number == self.attempt, !Task.isCancelled else {
					try self.account.recordClaim(handle: handle, at: now)
					return
				}
				self.pendingRecord = (handle, now)
				if self.apply(
					number,
					{
						$0.claiming = nil; $0.current = handle; $0.currentSince = now; $0
							.others = ([previous.current].compactMap(\.self) + previous.others).filter { $0 != handle }; $0.input = ""; $0
							.problem = nil; $0.status = .editing; $0.notice = nil },
				) {
					self.persistRecord()
				}
			} catch {
				if error as? HandleError == .taken {
					await self.answer(
						number,
						input: previous.input.isEmpty ? handle : previous.input,
						except: handle,
						notice: .taken(handle),
					); return
				}
				_ = self.apply(number) { state in
					state.claiming = nil
					switch error as? HandleError {
					case .notAllowed: state.notice = previous.others.contains(handle) ? .switchBackBlocked(
							handle,
							shorter: HandleRules.name(handle).count < state.minimum,
						) : .notAllowed(HandleRules.name(handle)); state.status = .editing
					case .tooSoon:
						let last = self.account.claimTimes.values.max(); let recent = last
							.flatMap { Date().timeIntervalSince($0) < 604_800 ? $0 : nil }
						state.notice = .tooSoon(changed: recent, next: recent?.addingTimeInterval(604_800)); state.input = ""; state
							.problem = nil; state.status = .editing
					case .limitReached: state.notice = .limitReached; state.input = ""; state.problem = nil; state.status = .editing
					default: state.notice = .claimFailed(handle)
					}
				}
			}
		}
	}
}

extension HandleStore {
	private func skip() {
		guard state.mode == .choose, state.claiming == nil, pendingRecord == nil,
		      sessionValid(origin.session), admit(origin) else { return }
		cancel()
		do {
			try persistSkip?()
			if apply(attempt, { $0.notice = nil }) {
				finish()
			}
		} catch { _ = apply(attempt) { $0.notice = .skipFailed } }
	}

	private func showMore() {
		if case let .suggestions(input, handles, shown, selected) = state.status {
			state.status = .suggestions(input: input, handles: handles, shown: min(shown + 3, handles.count), selected: selected)
		}
	}

	private func select(_ handle: String) {
		if state.editable, case let .suggestions(input, handles, shown, _) = state.status,
		   handles.prefix(shown).contains(handle)
		{
			state.status = .suggestions(input: input, handles: handles, shown: shown, selected: handle)
		}
	}
}
