//
//  LoggedInView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 11/22/22.
//

import SwiftUI
import XMTP

struct LoggedInView: View {
	var client: XMTP.Client

	var body: some View {
		NavigationView {
			VStack {
				ConversationListView(client: client)
				VStack(alignment: .leading) {
					Text("Connected as")
					Text("`\(client.address)`")
						.bold()
						.textSelection(.enabled)
				}
				.frame(maxWidth: .infinity)
				.font(.caption)
			}
		}
	}
}
