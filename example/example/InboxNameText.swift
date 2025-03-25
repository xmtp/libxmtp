import SwiftUI
import XMTPiOS

struct InboxNameText: View {
    @Environment(XmtpSession.self) private var session
    @Environment(NameResolver.self) private var names
    let inboxId: String
    var body: some View {
        let identity = session.inboxes[inboxId].value?.identities.first
        Text(names[identity].value ?? identity?.abbreviated ?? "")
    }
}
