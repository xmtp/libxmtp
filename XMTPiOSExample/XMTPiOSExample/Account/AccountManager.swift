//
//  AccountManager.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 11/22/22.
//

import Foundation
import XMTP

class AccountManager: ObservableObject {
	var account: Account

	init() {
		do {
			account = try Account.create()
		} catch {
			fatalError("Account could not be created: \(error)")
		}
	}
}
