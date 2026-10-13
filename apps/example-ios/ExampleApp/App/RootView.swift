import ExampleCore
import SwiftUI

struct RootView: View {
 @Bindable var graph: AppGraph
 var body: some View {
  NavigationStack(path: Binding(get: { graph.routes.path }, set: { graph.routes.setPath($0) })) {
   VStack(spacing: 24) { Image("Logo").resizable().scaledToFit().frame(width: 72, height: 72); Text("Join a server").font(.largeTitle.bold()); Text("Your server. Your conversations.").foregroundStyle(.secondary) }
    .navigationDestination(for: Route.self) { route in Text(String(describing: route)).navigationTitle("XMTP Messenger") }
  }.tint(Theme.tint).messengerMotion()
 }
}
