import Foundation
@testable import XmtpSdk

// verifies: CTYPE-008, CTYPE-009, CTYPE-027, CTYPE-029
func checkRetainedContent(_ failed: Message, nestedFailure: Message) throws {
    guard case let .custom(_, customRaw, nil, customDetails?) = failed.content,
          customRaw == failed.rawBytes, !customRaw.isEmpty,
          customDetails.code == "CodecDecodeFailed", customDetails.category == .callback,
          !customDetails.retryable, customDetails.message.contains("codec decode failed")
    else { throw ConformanceFailure("custom receive failure lost typed details or bytes") }
    guard case let .unknown(outerEncoded, outerRaw, outerDetails) = nestedFailure.content,
          outerRaw == nestedFailure.rawBytes, !outerRaw.isEmpty,
          outerEncoded?.fallback == nestedFailure.fallback,
          outerDetails.code == "CodecDecodeFailed", outerDetails.category == .callback, !outerDetails.retryable
    else { throw ConformanceFailure("nested host failure did not retain the outer reply") }

    let raw = Data([0xff, 0x80])
    let details = ErrorDetails(code: "MalformedEnvelope", category: .input, retryable: false, message: "invalid protobuf")
    var data = failed.data
    data.rawBytes = raw
    data.contentType = nil
    data.encoded = nil
    data.fallback = nil
    data.content = .unknown(encoded: nil, rawBytes: raw, error: details)
    let malformed = Message(data: data)
    guard malformed.contentType == nil, malformed.encoded == nil, malformed.rawBytes == raw,
          case let .unknown(malformedEncoded, preserved, cause) = malformed.content,
          malformedEncoded == nil, preserved == raw, cause.code == "MalformedEnvelope", cause.category == .input, !cause.retryable
    else { throw ConformanceFailure("malformed received content fabricated metadata") }

    guard let encoded = failed.encoded else { throw ConformanceFailure("custom codec input missing") }
    data.content = .text("valid reply")
    data.inReplyTo = ReplyParent(
        id: failed.id, senderInboxId: failed.senderInboxId, sentAt: failed.sentAt,
        kind: failed.kind, deliveryStatus: failed.deliveryStatus, rawBytes: failed.rawBytes,
        contentType: failed.contentType, fallback: failed.fallback, encoded: encoded,
        content: .custom(encoded: encoded, rawBytes: failed.rawBytes)
    )
    let withFailedParent = Message(data: data)
    guard case .standard(.text("valid reply")) = withFailedParent.content,
          case let .custom(_, parentRaw, nil, parentError?)? = withFailedParent.inReplyToContent,
          parentRaw == failed.rawBytes, parentError.code == "CodecDecodeFailed", parentError.category == .callback
    else { throw ConformanceFailure("parent host failure changed the reply") }
}
