//
//  GroupChat.swift
//
//
//  Created by Pat Nakajima on 6/11/23.
//

import Foundation

public enum GroupChat {
	public static func registerCodecs() {
		Client.register(codec: GroupChatMemberAddedCodec())
		Client.register(codec: GroupChatTitleChangedCodec())
	}
}
