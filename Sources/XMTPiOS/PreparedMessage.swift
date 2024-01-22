import CryptoKit
import Foundation

// This houses a fully prepared message that can be published
// as soon as the API client has connectivity.
//
// To support persistance layers that queue pending messages (e.g. while offline)
// this struct supports serializing to/from bytes that can be written to disk or elsewhere.
// See serializedData() and fromSerializedData()
public struct PreparedMessage {
    
    // The first envelope should send the message to the conversation itself.
    // Any more are for required intros/invites etc.
    // A client can just publish these when it has connectivity.
    public let envelopes: [Envelope]

    // Note: we serialize as a PublishRequest as a convenient `envelopes` wrapper.
    public static func fromSerializedData(_ serializedData: Data) throws -> PreparedMessage {
        let req = try Xmtp_MessageApi_V1_PublishRequest(serializedData: serializedData)
        return PreparedMessage(envelopes: req.envelopes)
    }

    // Note: we serialize as a PublishRequest as a convenient `envelopes` wrapper.
    public func serializedData() throws -> Data {
        let req = Xmtp_MessageApi_V1_PublishRequest.with { $0.envelopes = envelopes }
        return try req.serializedData()
    }

	public var messageID: String {
        Data(SHA256.hash(data: envelopes[0].message)).toHex
    }

    public var conversationTopic: String {
        envelopes[0].contentTopic
    }
}
