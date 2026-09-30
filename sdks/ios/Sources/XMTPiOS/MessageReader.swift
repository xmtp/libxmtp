import Foundation

public typealias DeliveryCursor = FfiDeliveryCursor
public typealias MessageCatchUpSnapshot = FfiMessageCatchUpSnapshot

/// Retained history and its gap-free stream boundary from one database snapshot.
public struct MessageHistorySnapshot {
	public let messages: [DecodedMessage]
	public let cursor: DeliveryCursor

	init(_ snapshot: FfiMessageHistorySnapshot) throws {
		messages = try snapshot.messages.compactMap { item in
			try DecodedMessage.decodeForDelivery(ffiMessage: item.message, deliveryCursor: item.cursor)
		}
		cursor = snapshot.cursor
	}
}

extension ConversationFilterType {
	var deliveryConversationType: FfiConversationType? {
		switch self {
		case .all: nil
		case .groups: .group
		case .dms: .dm
		}
	}
}

/// A default or replay reader with automatic next-item acknowledgement.
public final class MessageReader: @unchecked Sendable {
	private let ffiReader: FfiMessageReader
	private let receipt: MessageDeliveryStream
	private let pump: Task<Void, Never>

	init(_ ffiReader: FfiMessageReader) {
		self.ffiReader = ffiReader
		let receipt = MessageDeliveryStream(onClose: { ffiReader.end() })
		self.receipt = receipt
		pump = Task { [weak receipt] in
			do {
				while let item = try await ffiReader.nextDelivery() {
					guard let receipt else {
						item.acknowledgement.reject()
						ffiReader.end()
						return
					}
					receipt.receive(item)
				}
				receipt?.finish()
			} catch {
				receipt?.finish(error)
			}
		}
	}

	deinit {
		close()
	}

	/// A new request acknowledges the prior item. The returned item remains unacknowledged.
	public func next() async throws -> DecodedMessage? {
		try await receipt.next()
	}

	public func messages() -> AsyncThrowingStream<DecodedMessage, Error> {
		AsyncThrowingStream(unfolding: { [self] in
			try await next()
		})
	}

	/// Close without acknowledging the last item or any queued item.
	public func close() {
		receipt.finish()
		ffiReader.end()
		pump.cancel()
	}

	public func updateScope(conversationIds: [String]? = nil) throws {
		try ffiReader.updateScope(groupIds: conversationIds?.map(\.hexToData))
	}

	public func updateFilter(type: ConversationFilterType = .all, consentStates: [ConsentState]? = nil) {
		ffiReader.updateFilter(conversationType: type.deliveryConversationType, consentStates: consentStates?.toFFI)
	}

	public func catchUpSnapshot() -> MessageCatchUpSnapshot {
		ffiReader.catchUpSnapshot()
	}

	public func catchUpChanged() async -> MessageCatchUpSnapshot {
		await ffiReader.catchUpChanged()
	}
}
