import SwiftUI

struct MotionPolicy: Equatable, Sendable {
	var reduceMotion = false

	var animation: Animation {
		reduceMotion ? .easeInOut(duration: 0.2) : .spring(response: 0.28, dampingFraction: 0.86)
	}

	var transition: AnyTransition {
		reduceMotion ? .opacity : .opacity.combined(with: .scale(scale: 0.96))
	}
}

extension EnvironmentValues {
	@Entry var motionPolicy = MotionPolicy()
}

private struct MessengerMotion: ViewModifier {
	@Environment(\.accessibilityReduceMotion) private var reduceMotion

	func body(content: Content) -> some View {
		content.environment(\.motionPolicy, MotionPolicy(reduceMotion: reduceMotion))
	}
}

extension View {
	func messengerMotion() -> some View {
		modifier(MessengerMotion())
	}
}
