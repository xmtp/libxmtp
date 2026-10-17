import SwiftUI

struct MessageFlight {
	let id: String
	let text: String
}

struct MessageFlightAnchors: PreferenceKey {
	static var defaultValue: [String: Anchor<CGRect>] {
		[:]
	}

	static func reduce(value: inout [String: Anchor<CGRect>], nextValue: () -> [String: Anchor<CGRect>]) {
		value.merge(
			nextValue(),
			uniquingKeysWith: { _, next in next },
		)
	}
}

struct MessageFlightView: View {
	let text: String
	let source: CGRect
	let target: CGRect
	let motion: MotionPolicy
	let finished: () -> Void
	@State var landed = false
	var body: some View {
		Bubble(isOutgoing: true) { Text(text) }
			.frame(width: min(target.width, 300))
			.position(x: landed ? target.midX : source.midX, y: landed ? target.midY : source.midY)
			.scaleEffect(landed ? 1 : 0.92)
			.onAppear {
				withAnimation(motion.animation, completionCriteria: .logicallyComplete) { landed = true } completion: { finished()
				}
			}
	}
}
