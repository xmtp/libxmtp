import Foundation

/// Holds the last stream without preventing iterator-drop cleanup.
final class StreamHolder: @unchecked Sendable {
	private let lock = NSLock()
	private weak var stream: MessageDeliveryStream?

	func setStream(_ stream: MessageDeliveryStream) {
		lock.lock()
		self.stream = stream
		lock.unlock()
	}

	func end() {
		lock.lock()
		let current = stream
		lock.unlock()
		current?.finish()
	}
}

private final class MessageDeliveryCallback: FfiMessageCallback, @unchecked Sendable {
	private weak var stream: MessageDeliveryStream?

	init(_ stream: MessageDeliveryStream) {
		self.stream = stream
	}

	func onMessage(delivery: FfiMessageDelivery) throws {
		guard let stream else {
			delivery.acknowledgement.reject()
			return
		}
		stream.receive(delivery)
	}

	func onError(error: FfiError) {
		stream?.finish(error)
	}

	func onClose() {
		stream?.finish()
	}
}

enum MessageDeliveryStreamError: Error, Equatable {
	case queueFull
	case concurrentNext
	case decodeFailed
}

protocol MessageDeliveryToken: AnyObject {
	func checkOwner() throws -> Bool
	func acknowledge() throws
	func reject()
}

extension FfiDeliveryAcknowledgement: MessageDeliveryToken {}

struct QueuedMessageDelivery {
	let message: FfiMessage
	let cursor: FfiDeliveryCursor
	let acknowledgement: any MessageDeliveryToken
}

/// A one-item receipt mailbox. Only `next` starts an app handoff or acknowledges a prior handoff.
final class MessageDeliveryStream: @unchecked Sendable {
	private let lock = NSLock()
	private var queued: QueuedMessageDelivery?
	private var active: (any MessageDeliveryToken)?
	private var handedOff = false
	private var nextInProgress = false
	private var waiter: CheckedContinuation<QueuedMessageDelivery?, Error>?
	private var ended = false
	private var failure: Error?
	private var stream: FfiStreamCloser?
	private var starting: Task<Void, Never>?
	private let onClose: (() -> Void)?

	init(onClose: (() -> Void)?) {
		self.onClose = onClose
	}

	deinit {
		finish()
	}

	func start(_ open: @escaping @Sendable (FfiMessageCallback) async -> FfiStreamCloser) {
		let callback = MessageDeliveryCallback(self)
		let task = Task { [weak self] in
			let opened = await open(callback)
			guard let self else {
				opened.end()
				return
			}
			install(opened)
		}
		lock.lock()
		let cancelled = ended
		if !cancelled {
			starting = task
		}
		lock.unlock()
		if cancelled {
			task.cancel()
		}
	}

	private func install(_ opened: FfiStreamCloser) {
		lock.lock()
		let cancelled = ended
		if !cancelled {
			stream = opened
		}
		starting = nil
		lock.unlock()
		if cancelled {
			opened.end()
		}
	}

	func receive(_ delivery: FfiMessageDelivery) {
		receive(QueuedMessageDelivery(
			message: delivery.message,
			cursor: delivery.cursor,
			acknowledgement: delivery.acknowledgement
		))
	}

	func receive(_ delivery: QueuedMessageDelivery) {
		lock.lock()
		if ended {
			lock.unlock()
			delivery.acknowledgement.reject()
			return
		}
		if let waiting = waiter {
			waiter = nil
			active = delivery.acknowledgement
			lock.unlock()
			waiting.resume(returning: delivery)
		} else if queued == nil {
			queued = delivery
			lock.unlock()
		} else {
			let terminate = finishLocked(MessageDeliveryStreamError.queueFull)
			lock.unlock()
			delivery.acknowledgement.reject()
			terminate?()
		}
	}

	func next() async throws -> DecodedMessage? {
		try await withTaskCancellationHandler {
			do {
				try beginNext()
				defer { endNext() }
				try Task.checkCancellation()
				if let previous = takePrevious() {
					try previous.acknowledge()
					clearActive(reject: false)
				}
				while let delivery = try await receiveNext() {
					try Task.checkCancellation()
					guard let decoded = DecodedMessage.create(
						ffiMessage: delivery.message,
						deliveryCursor: delivery.cursor
					) else {
						throw MessageDeliveryStreamError.decodeFailed
					}
					// Decode before dispatch. A scope change during decoding must still exclude this item.
					guard try delivery.acknowledgement.checkOwner() else {
						clearActive(reject: true)
						continue
					}
					try Task.checkCancellation()
					guard markHandedOff() else {
						throw CancellationError()
					}
					return decoded
				}
				return nil
			} catch {
				finish(error)
				throw error
			}
		} onCancel: {
			self.finish(CancellationError())
		}
	}

	private func beginNext() throws {
		lock.lock()
		defer { lock.unlock() }
		guard !nextInProgress else {
			throw MessageDeliveryStreamError.concurrentNext
		}
		nextInProgress = true
	}

	private func endNext() {
		lock.lock()
		nextInProgress = false
		lock.unlock()
	}

	private func takePrevious() -> (any MessageDeliveryToken)? {
		lock.lock()
		defer { lock.unlock() }
		guard !ended, handedOff else { return nil }
		let previous = active
		// Keep ownership until acknowledgement succeeds so finish can reject a failed attempt.
		handedOff = false
		return previous
	}

	private func clearActive(reject: Bool) {
		lock.lock()
		let previous = active
		active = nil
		handedOff = false
		lock.unlock()
		if reject {
			previous?.reject()
		}
	}

	private func markHandedOff() -> Bool {
		lock.lock()
		defer { lock.unlock() }
		guard !ended, active != nil else { return false }
		handedOff = true
		return true
	}

	private func receiveNext() async throws -> QueuedMessageDelivery? {
		try await withCheckedThrowingContinuation { continuation in
			lock.lock()
			if ended {
				let error = failure
				lock.unlock()
				if let error {
					continuation.resume(throwing: error)
				} else {
					continuation.resume(returning: nil)
				}
			} else if let delivery = queued {
				queued = nil
				active = delivery.acknowledgement
				lock.unlock()
				continuation.resume(returning: delivery)
			} else if waiter != nil {
				lock.unlock()
				continuation.resume(throwing: MessageDeliveryStreamError.concurrentNext)
			} else {
				waiter = continuation
				lock.unlock()
			}
		}
	}

	func finish(_ error: Error? = nil) {
		lock.lock()
		let terminate = finishLocked(error)
		lock.unlock()
		terminate?()
	}

	/// Record the terminal state before releasing the mailbox lock. Run cleanup outside the lock.
	private func finishLocked(_ error: Error?) -> (() -> Void)? {
		guard !ended else { return nil }
		ended = true
		failure = error
		let pending = queued?.acknowledgement
		let current = active
		let waiting = waiter
		let opened = stream
		let task = starting
		let onClose = onClose
		queued = nil
		active = nil
		handedOff = false
		waiter = nil
		stream = nil
		starting = nil
		return {
			task?.cancel()
			opened?.end()
			pending?.reject()
			current?.reject()
			if let error {
				waiting?.resume(throwing: error)
			} else {
				waiting?.resume(returning: nil)
			}
			onClose?()
		}
	}
}

func messageDeliveryStream(
	holder: StreamHolder? = nil,
	onClose: (() -> Void)?,
	open: @escaping @Sendable (FfiMessageCallback) async -> FfiStreamCloser
) -> AsyncThrowingStream<DecodedMessage, Error> {
	let receipt = MessageDeliveryStream(onClose: onClose)
	holder?.setStream(receipt)
	receipt.start(open)
	return AsyncThrowingStream(unfolding: {
		try await receipt.next()
	})
}
