import ExampleCore
import SwiftUI

extension ChatContentView {
	var requestBar: some View {
		VStack(spacing: 12) {
			Image(systemName: "person.crop.circle.badge.questionmark").font(.title2).foregroundStyle(Theme.tint)
				.accessibilityHidden(true)
			Text("Message request", tableName: "Chat").font(.headline)
			Text("Accept to reply. Request media stays closed until you choose to download it.", tableName: "Chat")
				.font(.subheadline).foregroundStyle(Theme.secondaryLabel).multilineTextAlignment(.center)
			Button { graph.navigate(.details(id)) } label: {
				HStack { Text("Sender details", tableName: "Chat"); Spacer(); Image(systemName: "chevron.right") }
			}.buttonStyle(.glass).frame(maxWidth: .infinity)
			Button {
				graph.requests?.send(.accept(id))
				runtime.timeline.send(.refresh)
			} label: { Text("Accept", tableName: "Chat").frame(maxWidth: .infinity) }.buttonStyle(.glassProminent)
			HStack(spacing: 12) {
				Button(role: .destructive) { graph.requests?.send(.requestDelete(id)) } label: {
					Text("Delete request", tableName: "Chat").frame(maxWidth: .infinity)
				}.buttonStyle(.glass)
				Button(role: .destructive) { graph.requests?.send(.requestBlock(id)) } label: {
					Text("Block", tableName: "Chat").frame(maxWidth: .infinity)
				}.buttonStyle(.glass)
			}
		}.padding(16).frame(maxWidth: .infinity)
			.background(Theme.incomingBubble, in: RoundedRectangle(cornerRadius: 24))
			.padding(.horizontal, 16).padding(.bottom, 12)
			.confirmationDialog("Confirm request action", isPresented: Binding(
				get: { graph.requests?.state.confirmation != nil },
				set: {
					if !$0 {
						graph.requests?.send(.cancel)
					}
				},
			)) { Button(String(localized: "Confirm", table: "Chat"), role: .destructive) { graph.requests?.send(.confirm) } }
	}
}
