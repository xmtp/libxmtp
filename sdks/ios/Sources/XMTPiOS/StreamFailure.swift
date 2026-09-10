import Foundation

public typealias StreamFailureKind = FfiStreamFailureKind
public typealias StreamBarrierReason = FfiStreamBarrierReason
public typealias StreamBarrierCauseKind = FfiStreamBarrierCauseKind
public typealias StreamBarrierCause = FfiStreamBarrierCause
public typealias StreamBarrierTopic = FfiStreamBarrierTopic
public typealias StreamBarrierFailure = FfiStreamBarrierFailure
public typealias StreamFailureDetails = FfiStreamFailureDetails

public extension Error {
	/// Structured barrier, publish-confirmation, or catch-up failure details.
	/// A nil target means capture failed. Zero is a captured empty target.
	/// Cursors and counts keep their full unsigned 64-bit values.
	var streamFailureDetails: StreamFailureDetails? {
		readStreamFailureDetails(self)
	}
}

func readStreamFailureDetails(
	_ error: Error,
	decode: (String) -> StreamFailureDetails? = {
		getStreamFailureDetails(errorMessage: $0)
	}
) -> StreamFailureDetails? {
	guard let error = error as? FfiError else { return nil }
	switch error {
	case let .Error(message):
		return decode(message)
	}
}
