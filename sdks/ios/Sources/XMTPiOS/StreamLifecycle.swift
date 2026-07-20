import Foundation
import os

#if canImport(UIKit)
	import UIKit
#endif

/// Counts of what a ``Client/catchUpToLive(timeoutMs:)`` run brought into the
/// local store, plus whether it reached the live edge.
public struct CatchUpSummary: Sendable {
	/// Messages newly persisted during this run.
	public let messages: UInt64
	/// Conversations newly joined during this run.
	public let conversations: UInt64
	/// `true` if catch-up reached the live edge; `false` if a `timeoutMs`
	/// deadline cut it short. Partial progress is persisted either way.
	public let completed: Bool

	init(_ ffi: FfiCatchUpSummary) {
		messages = ffi.messages
		conversations = ffi.conversations
		completed = ffi.completed
	}
}

/// Keeps the process-shared streaming wire in step with app foreground /
/// background.
///
/// The streaming transport is shared across every ``Client`` in the process, so
/// suspend/resume are process-global, not per-client. This is registered **once
/// per process** and never torn down — it lives for the process and captures no
/// client state, which is why there's no matching deregistration hook to get
/// wrong.
///
/// Two design points worth stating, because both are easy to get wrong:
///
/// - **Scenes are already aggregated.** We observe the `UIApplication`-level
///   notifications, which UIKit posts only at the app boundary:
///   `didEnterBackground` fires when the *last* scene backgrounds, and
///   `willEnterForeground` when the *first* scene foregrounds. So there is no
///   per-scene counting to do here — UIKit has done it. (Per-scene control
///   would mean observing `UIScene.*` and counting; we deliberately don't.)
///
/// - **Transitions are reconciled, not fired.** Each notification only records
///   the desired state; a single serial reconciler drives the wire toward it,
///   one operation at a time. `resumeStreams()` is unbounded while offline and
///   uniffi does not propagate task cancellation, so a naive
///   `Task { await resume }` / `Task { await suspend }` per notification could
///   complete out of order and strand the wire live-while-backgrounded. The
///   reconciler instead re-checks the desired state after every applied op and
///   issues a correcting op if it changed, converging to the last intent.
///
/// **Known limitation — background launch.** State is seeded to foreground and
/// only reacts to *transitions*; a process launched straight into the
/// background (silent push / `BGTask`) fires no `didEnterBackground`, so if it
/// opens live streams and never foregrounds, the wire can stay live in the
/// background. We can't seed from `UIApplication.shared.applicationState`
/// without breaking app-extension builds (`.shared` is extension-unavailable).
/// Backgrounds that only catch up — the normal pattern — are unaffected, since
/// ``Client/catchUpToLive(timeoutMs:)`` uses its own connection, not this wire.
/// The complete fix is for the shared transport to honor a suspend requested
/// before it is first opened (a libxmtp follow-on).
final class StreamLifecycleManager: @unchecked Sendable {
	static let shared = StreamLifecycleManager()

	private let lock = NSLock()
	private var isRegistered = false
	/// What app lifecycle wants the wire to be. Starts foreground.
	private var desiredLive = true
	/// What we last drove the wire to. Streams open live at client creation.
	private var appliedLive = true
	/// Whether a reconciler task is currently draining toward `desiredLive`.
	private var isReconciling = false

	private init() {}

	func enableIfNeeded() {
		#if canImport(UIKit)
			lock.lock()
			defer { lock.unlock() }
			guard !isRegistered else { return }
			isRegistered = true

			let center = NotificationCenter.default
			// Tokens intentionally discarded: these observers live for the
			// process and capture only this singleton.
			_ = center.addObserver(
				forName: UIApplication.didEnterBackgroundNotification,
				object: nil, queue: .main
			) { [weak self] _ in
				self?.setDesired(live: false)
			}
			_ = center.addObserver(
				forName: UIApplication.willEnterForegroundNotification,
				object: nil, queue: .main
			) { [weak self] _ in
				self?.setDesired(live: true)
			}
		#endif
	}

	private func setDesired(live: Bool) {
		lock.lock()
		desiredLive = live
		let shouldStart = !isReconciling && appliedLive != desiredLive
		if shouldStart { isReconciling = true }
		lock.unlock()

		if shouldStart {
			Task { await reconcile() }
		}
	}

	/// Drives the wire toward `desiredLive`, one op at a time, until they agree.
	/// Applying an op is async and may be slow (`resumeStreams` blocks while
	/// offline); the loop re-reads `desiredLive` afterward so a flip mid-op is
	/// corrected rather than lost. The lock is only touched by the synchronous
	/// helpers — never held across an `await`.
	///
	/// A failed op does *not* advance `appliedLive`: recording a transition that
	/// never happened would leave the wire stuck in the wrong state with no retry.
	/// Instead the reconciler stops and leaves `appliedLive` misaligned, so the
	/// next foreground/background transition re-runs the op it was owed.
	private func reconcile() async {
		while let target = nextTarget() {
			do {
				if target {
					try await resumeStreams()
				} else {
					try await suspendStreams()
				}
			} catch {
				os_log(
					"Stream %{public}@ failed; retrying on the next lifecycle transition: %{public}@",
					log: OSLog.default, type: .error,
					target ? "resume" : "suspend", error.localizedDescription
				)
				stopReconciling()
				return
			}
			markApplied(target)
		}
	}

	/// The next state to apply, or `nil` when the wire already matches intent
	/// (clearing the reconciling flag so the next transition restarts the loop).
	private func nextTarget() -> Bool? {
		lock.lock()
		defer { lock.unlock() }
		guard desiredLive != appliedLive else {
			isReconciling = false
			return nil
		}
		return desiredLive
	}

	private func markApplied(_ live: Bool) {
		lock.lock()
		defer { lock.unlock() }
		appliedLive = live
	}

	/// Clears the reconciling flag without advancing `appliedLive`, so a later
	/// transition restarts the loop and retries the op that failed.
	private func stopReconciling() {
		lock.lock()
		defer { lock.unlock() }
		isReconciling = false
	}
}
