import SwiftUI
import SwiftData
import XMTPiOS
import OSLog

// Initially, the App handles getting the user logged-in.
//
// But after login, the `Router` in the `HomeView` takes over navigation.
@main
struct exampleApp: App {
    let session = XmtpSession()
    let names = NameResolver()
    let router = Router()

    var body: some Scene {
        WindowGroup {
            switch session.state {
            case .loading:
                ProgressView()
            case .loggedOut:
                LoginView()
                    .environment(session)
                    .environment(router)
                    .environment(names)
            case .loggedIn:
                HomeView()
                    .environment(session)
                    .environment(router)
                    .environment(names)
            }
        }
    }
}

// Present the login options for the user.
private struct LoginView: View {
    @Environment(XmtpSession.self) var session
    @State var isLoggingIn = false
    var body: some View {

        // TODO: support more login methods
        Button("Login (random account)") {
            isLoggingIn = true
            Task {
                defer {
                    isLoggingIn = false
                }
                try await session.login()
            }
        }
        .disabled(isLoggingIn)
    }
}
