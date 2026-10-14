import SwiftUI

private struct MessengerGlass<S: Shape>: ViewModifier {
	@Environment(\.accessibilityReduceTransparency) private var reduceTransparency
	@Environment(\.colorSchemeContrast) private var contrast
	let shape: S

	func body(content: Content) -> some View {
		if reduceTransparency {
			content.background(.background, in: shape)
				.overlay(shape.stroke(.primary.opacity(contrast == .increased ? 0.6 : 0.2)))
		} else {
			content.glassEffect(.regular, in: shape)
		}
	}
}

extension View {
	/// For custom chrome such as the composer. Buttons use native .glass styles.
	func messengerGlass(in shape: some Shape) -> some View {
		modifier(MessengerGlass(shape: shape))
	}
}
