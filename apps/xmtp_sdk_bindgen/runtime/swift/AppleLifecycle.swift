import Foundation
import os
#if canImport(UIKit)
import UIKit
#endif

public extension SDKClient {
    /// Enable automatic process-wide stream suspension before the first client opens.
    nonisolated(unsafe) static var manageStreamLifecycle = true
}

enum AppleStreamLifecycle {
    static func enableIfNeeded() {
        if SDKClient.manageStreamLifecycle {
            StreamLifecycleManager.shared.enableIfNeeded()
        }
    }
}

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
			if isRegistered {
				lock.unlock()
				return
			}
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
			lock.unlock()

			// A process launched straight into the background fires no
			// `didEnterBackground`; seed the backgrounded case so a stream opened
			// before any foreground is born parked. `setDesired` takes the lock,
			// so this must run after unlocking. `nil` (can't tell / app extension)
			// leaves the foreground default.
			if launchedInBackground() == true {
				setDesired(live: false)
			}
		#endif
	}

	/// Whether the host app is currently backgrounded, or `nil` when it can't be
	/// determined — an app extension (no app-level lifecycle to seed from) or an
	/// unexpected runtime shape. XmtpSdk is usable from extension targets, where
	/// `UIApplication.shared` is compile-time unavailable, so this reads the
	/// shared application and its state dynamically and only in the app process.
	/// Any failure returns `nil`, leaving the foreground default — it can never
	/// regress a normally-foregrounded app.
	private func launchedInBackground() -> Bool? {
		#if canImport(UIKit)
			guard
				Bundle.main.bundleURL.pathExtension != "appex",
				let appClass = NSClassFromString("UIApplication")
			else { return nil }
			// Read `+[UIApplication sharedApplication].applicationState` dynamically
			// through the ObjC runtime, so nothing references the
			// extension-unavailable `.shared` at compile time.
			let selector = NSSelectorFromString("sharedApplication")
			guard
				(appClass as AnyObject).responds(to: selector),
				let shared = (appClass as AnyObject).perform(selector)?.takeUnretainedValue(),
				let state = (shared as AnyObject).value(forKey: "applicationState") as? Int
			else { return nil }
			return state == 2 // UIApplication.State.background
		#else
			return nil
		#endif
	}

	private func setDesired(live: Bool) {
		lock.lock()
		desiredLive = live
		let shouldStart = !isReconciling && appliedLive != desiredLive
		if shouldStart {
			isReconciling = true
		}
		lock.unlock()

		if shouldStart {
			Task { await reconcile() }
		}
	}

	/// Drives the wire toward `desiredLive`, one op at a time, until they agree.
	/// Applying an op is async and may take a moment (`suspendStreams` awaits
	/// the wire's release; `resumeStreams` returns once the resume is enqueued);
	/// the loop re-reads `desiredLive` afterward so a flip mid-op is corrected
	/// rather than lost. The lock is only touched by the synchronous helpers —
	/// never held across an `await`.
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
