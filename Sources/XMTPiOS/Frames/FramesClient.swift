//
//  FramesClient.swift
//  
//
//  Created by Alex Risch on 3/28/24.
//

import Foundation
import LibXMTP

public typealias FrameActionBody = Xmtp_MessageContents_FrameActionBody
public typealias FrameAction = Xmtp_MessageContents_FrameAction

enum FramesClientError: Error {
    case missingConversationTopic
    case missingTarget
    case readMetadataFailed(message: String, code: Int)
    case postFrameFailed(message: String, code: Int)
}

public class FramesClient {
    var xmtpClient: Client
    public var proxy: OpenFramesProxy

    public init(xmtpClient: Client, proxy: OpenFramesProxy? = nil) {
        self.xmtpClient = xmtpClient
        self.proxy = proxy ?? OpenFramesProxy()
    }

    public func signFrameAction(inputs: FrameActionInputs) async throws -> FramePostPayload {
        let opaqueConversationIdentifier = try self.buildOpaqueIdentifier(inputs: inputs)
        let frameUrl = inputs.frameUrl
        let buttonIndex = inputs.buttonIndex
        let inputText = inputs.inputText ?? ""
        let state = inputs.state ?? ""
        let now = Date().timeIntervalSince1970
        let timestamp = now

        var toSign = FrameActionBody()
        toSign.frameURL = frameUrl
        toSign.buttonIndex = buttonIndex
        toSign.opaqueConversationIdentifier =  opaqueConversationIdentifier
        toSign.timestamp = UInt64(timestamp)
        toSign.inputText = inputText
        toSign.unixTimestamp = UInt32(now)
        toSign.state = state

        let signedAction = try await self.buildSignedFrameAction(actionBodyInputs: toSign)

        let untrustedData = FramePostUntrustedData(
            url: frameUrl, timestamp: UInt64(now), buttonIndex: buttonIndex, inputText: inputText, state: state, walletAddress: self.xmtpClient.address, opaqueConversationIdentifier: opaqueConversationIdentifier, unixTimestamp: UInt32(now)
        )


        let trustedData = FramePostTrustedData(messageBytes: signedAction.base64EncodedString())

        let payload = FramePostPayload(
            clientProtocol: "xmtp@\(PROTOCOL_VERSION)", untrustedData: untrustedData, trustedData: trustedData
        )

        return payload
    }
    
    private func signDigest(digest: Data) async throws -> Signature {
        let key = self.xmtpClient.keys.identityKey
        let privateKey = try PrivateKey(key)
        let signature = try await privateKey.sign(Data(digest))
        return signature
    }
    
    private func getPublicKeyBundle() async throws -> PublicKeyBundle {
        let bundleBytes = self.xmtpClient.publicKeyBundle;
        return try PublicKeyBundle(bundleBytes);
      }
    
    private func buildSignedFrameAction(actionBodyInputs: FrameActionBody) async throws -> Data {

        let digest = sha256(input: try actionBodyInputs.serializedData())
        let signature = try await self.signDigest(digest: digest)

        let publicKeyBundle = try await self.getPublicKeyBundle()
        var frameAction = FrameAction()
        frameAction.actionBody = try actionBodyInputs.serializedData()
        frameAction.signature = signature
        frameAction.signedPublicKeyBundle = try SignedPublicKeyBundle(publicKeyBundle)
        
        return try frameAction.serializedData()
    }
    
    private func buildOpaqueIdentifier(inputs: FrameActionInputs) throws -> String {
        switch inputs.conversationInputs {
        case .group(let groupInputs):
            let combined = groupInputs.groupId + groupInputs.groupSecret
            let digest = sha256(input: combined)
            return digest.base64EncodedString()
        case .dm(let dmInputs):
            guard let conversationTopic = dmInputs.conversationTopic else {
                throw FramesClientError.missingConversationTopic
            }
            guard let combined = (conversationTopic.lowercased() + dmInputs.participantAccountAddresses.map { $0.lowercased() }.sorted().joined()).data(using: .utf8) else {
                throw FramesClientError.missingConversationTopic
            }
            let digest = sha256(input: combined)
            return digest.base64EncodedString()
        }
    }

}
