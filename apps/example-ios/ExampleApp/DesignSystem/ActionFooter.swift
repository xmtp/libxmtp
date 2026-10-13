import SwiftUI

/// Bottom actions rely on the system safe area, without adding a second bottom inset.
struct ActionFooter<Content: View>: View {
	@ViewBuilder let content: () -> Content

	var body: some View {
		VStack(spacing: 10, content: content)
			.padding(.horizontal, 20)
			.padding(.top, 8)
			.frame(maxWidth: 560)
			.frame(maxWidth: .infinity)
	}
}
