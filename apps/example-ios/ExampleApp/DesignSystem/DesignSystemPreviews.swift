import SwiftUI

private struct DesignSystemPreview: View {
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                Text("Chats").font(.largeTitle.bold())
                HStack {
                    Avatar(seed: "protocol", label: "PD")
                    Avatar(seed: "maya", label: "M")
                    Avatar(seed: "release", label: "RC")
                    Avatar(seed: "9c", label: "9c", monospaced: true)
                }
                VStack(alignment: .leading) {
                    HandleText(handle: "marco#0412").font(.headline)
                    IdentityText(inboxID: "3f9a21c0e4b17d2a", handle: nil)
                }
                Bubble(isOutgoing: false) {
                    Text("Pushed the key package rotation.")
                }
                .overlay(alignment: .topTrailing) {
                    ReactionBadge(emoji: "👍", count: 3).offset(y: -12)
                }
                HStack {
                    Spacer(minLength: 24)
                    Bubble(isOutgoing: true) { Text("Looking now.") }
                }
                HStack {
                    Button("Join a server") {}.buttonStyle(.glassProminent)
                    Button("Retry") {}.buttonStyle(.glass)
                }
                Button("Delete request", role: .destructive) {}.buttonStyle(.glass)
                HStack {
                    Image(systemName: Symbol.search).accessibilityHidden(true)
                    Text("Search")
                    Spacer()
                }
                .padding()
                .messengerGlass(in: Capsule())
            }
            .padding()
        }
        .tint(Theme.tint)
        .messengerMotion()
    }
}

#Preview("Foundations · Light") {
    DesignSystemPreview().preferredColorScheme(.light)
}

#Preview("Foundations · Dark") {
    DesignSystemPreview().preferredColorScheme(.dark)
}

#Preview("Foundations · AX3") {
    DesignSystemPreview().environment(\.dynamicTypeSize, .accessibility3)
}

#Preview("Foundations · AX5 Dark") {
    DesignSystemPreview()
        .environment(\.dynamicTypeSize, .accessibility5)
        .preferredColorScheme(.dark)
}
