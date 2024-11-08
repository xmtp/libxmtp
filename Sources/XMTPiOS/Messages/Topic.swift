//
//  Topic.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

public enum Topic {
	case userWelcome(String),
		 groupMessage(String)

	var description: String {
		switch self {
		case let .groupMessage(groupId):
			return wrapMls("g-\(groupId)")
		case let .userWelcome(installationId):
			return wrapMls("w-\(installationId)")
		}
	}
	
	private func wrapMls(_ value: String) -> String {
		"/xmtp/mls/1/\(value)/proto"
	}

    static func isValidTopic(topic: String) -> Bool {
        return topic.allSatisfy(\.isASCII)
    }
}
