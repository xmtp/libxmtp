//
//  File.swift
//  
//
//  Created by Alex Risch on 3/28/24.
//

import Foundation

typealias AcceptedFrameClients = [String: String]

enum OpenFrameButton: Codable {
    case link(target: String, label: String)
    case mint(target: String, label: String)
    case post(target: String?, label: String)
    case postRedirect(target: String?, label: String)

    enum CodingKeys: CodingKey {
        case action, target, label
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let action = try container.decode(String.self, forKey: .action)
        guard let target = try container.decodeIfPresent(String.self, forKey: .target) else {
            throw FramesClientError.missingTarget
        }
        let label = try container.decode(String.self, forKey: .label)

        switch action {
        case "link":
            self = .link(target: target, label: label)
        case "mint":
            self = .mint(target: target, label: label)
        case "post":
            self = .post(target: target, label: label)
        case "post_redirect":
            self = .postRedirect(target: target, label: label)
        default:
            throw DecodingError.dataCorruptedError(forKey: .action, in: container, debugDescription: "Invalid action value")
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .link(let target, let label):
            try container.encode("link", forKey: .action)
            try container.encode(target, forKey: .target)
            try container.encode(label, forKey: .label)
        case .mint(let target, let label):
            try container.encode("mint", forKey: .action)
            try container.encode(target, forKey: .target)
            try container.encode(label, forKey: .label)
        case .post(let target, let label):
            try container.encode("post", forKey: .action)
            try container.encode(target, forKey: .target)
            try container.encode(label, forKey: .label)
        case .postRedirect(let target, let label):
            try container.encode("post_redirect", forKey: .action)
            try container.encode(target, forKey: .target)
            try container.encode(label, forKey: .label)
        }
    }
}

public struct OpenFrameImage: Codable {
    let content: String
    let aspectRatio: AspectRatio?
    let alt: String?
}

public enum AspectRatio: String, Codable {
    case ratio_1_91_1 = "1.91.1"
    case ratio_1_1 = "1:1"
}

public struct TextInput: Codable {
    let content: String
}

struct OpenFrameResult: Codable {
    let acceptedClients: AcceptedFrameClients
    let image: OpenFrameImage
    let postUrl: String?
    let textInput: TextInput?
    let buttons: [String: OpenFrameButton]?
    let ogImage: String
    let state: String?
};

public struct GetMetadataResponse: Codable {
    let url: String
    public let extractedTags: [String: String]
}

public struct PostRedirectResponse: Codable  {
    let originalUrl: String
    let redirectedTo: String
};

public struct OpenFramesUntrustedData: Codable {
    let url: String
        let timestamp: Int
        let buttonIndex: Int
        let inputText: String?
        let state: String?
}

public typealias FramesApiRedirectResponse = PostRedirectResponse;

public struct FramePostUntrustedData: Codable {
    let url: String
    let timestamp: UInt64
    let buttonIndex: Int32
    let inputText: String?
    let state: String?
    let walletAddress: String
    let opaqueConversationIdentifier: String
    let unixTimestamp: UInt32
}

public struct FramePostTrustedData: Codable {
    let messageBytes: String
}

public struct FramePostPayload: Codable {
    let clientProtocol: String
    let untrustedData: FramePostUntrustedData
    let trustedData: FramePostTrustedData
}

public struct DmActionInputs: Codable {
    public let conversationTopic: String?
    public let participantAccountAddresses: [String]
    public init(conversationTopic: String? = nil, participantAccountAddresses: [String]) {
        self.conversationTopic = conversationTopic
        self.participantAccountAddresses = participantAccountAddresses
    }
}

public struct GroupActionInputs: Codable {
    let groupId: Data
    let groupSecret: Data
}

public enum ConversationActionInputs: Codable {
    case dm(DmActionInputs)
    case group(GroupActionInputs)
}

public struct FrameActionInputs: Codable {
    let frameUrl: String
    let buttonIndex: Int32
    let inputText: String?
    let state: String?
    let conversationInputs: ConversationActionInputs
    public init(frameUrl: String, buttonIndex: Int32, inputText: String?, state: String?, conversationInputs: ConversationActionInputs) {
        self.frameUrl = frameUrl
        self.buttonIndex = buttonIndex
        self.inputText = inputText
        self.state = state
        self.conversationInputs = conversationInputs
    }
}

