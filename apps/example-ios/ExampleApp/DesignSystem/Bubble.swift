import SwiftUI

/// Opaque content surface. Only the last bubble in a run needs a tail.
struct Bubble<Content: View>: View {
	let isOutgoing: Bool
	var showsTail = true
	@ViewBuilder let content: () -> Content

	var body: some View {
		content()
			.padding(.horizontal, 14)
			.padding(.vertical, 10)
			.padding(isOutgoing ? .trailing : .leading, showsTail ? 5 : 0)
			.foregroundStyle(isOutgoing ? Color.white : Color.primary)
			.background(isOutgoing ? Theme.tint : Theme.incomingBubble,
			            in: BubbleShape(isOutgoing: isOutgoing, showsTail: showsTail))
	}
}

struct BubbleShape: Shape {
	var isOutgoing: Bool
	var showsTail: Bool

	func path(in rect: CGRect) -> Path {
		let tail: CGFloat = showsTail ? 6 : 0
		let body = CGRect(x: isOutgoing ? 0 : tail, y: 0,
		                  width: max(0, rect.width - tail), height: rect.height)
		var path = Path(roundedRect: body, cornerRadius: min(18, rect.height / 2))
		if showsTail {
			var tip = Path()
			tip.move(to: CGPoint(x: body.maxX - 14, y: rect.maxY - 13))
			tip.addQuadCurve(to: CGPoint(x: rect.maxX, y: rect.maxY),
			                 control: CGPoint(x: rect.maxX - 9, y: rect.maxY))
			tip.addQuadCurve(to: CGPoint(x: body.maxX - 17, y: rect.maxY - 2),
			                 control: CGPoint(x: body.maxX - 8, y: rect.maxY + 1))
			tip.closeSubpath()
			if !isOutgoing {
				tip = tip.applying(CGAffineTransform(a: -1, b: 0, c: 0, d: 1,
				                                     tx: rect.minX + rect.maxX, ty: 0))
			}
			path.addPath(tip)
		}
		return path
	}
}

struct ReactionBadge: View {
	let emoji: String
	let count: Int
	var isSelected = false

	var body: some View {
		HStack(spacing: 3) {
			Text(verbatim: emoji)
			if count > 1 {
				Text(count, format: .number)
			}
		}
		.font(.caption)
		.padding(.horizontal, 7)
		.padding(.vertical, 4)
		.foregroundStyle(isSelected ? Theme.accentText : .primary)
		.background(isSelected ? Theme.tint.opacity(0.15) : Theme.incomingBubble, in: Capsule())
		.overlay(Capsule().stroke(.background, lineWidth: 2))
		.accessibilityElement(children: .ignore)
		.accessibilityLabel(Text(verbatim: "\(emoji), \(count)"))
		.accessibilityAddTraits(isSelected ? .isSelected : [])
	}
}
