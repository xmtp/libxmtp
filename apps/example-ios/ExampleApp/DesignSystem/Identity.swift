import CryptoKit
import SwiftUI

struct Avatar: View {
	let seed: String
	let label: String
	var size: CGFloat = 44
	var monospaced = false
	@ScaledMetric(relativeTo: .body) private var scale: CGFloat = 1

	private var palette: Int {
		Int(Array(SHA256.hash(data: Data(seed.utf8))).first ?? 0) % 6
	}

	var body: some View {
		Text(verbatim: label)
			.font(monospaced ? .headline.monospaced() : .headline)
			.foregroundStyle(Color("AvatarForeground\(palette)"))
			.frame(width: size * scale, height: size * scale)
			.background(Color("AvatarBackground\(palette)"), in: Circle())
			.accessibilityHidden(true)
	}
}

struct HandleText: View {
	let handle: String

	private var normalized: String {
		handle.hasPrefix("@") ? String(handle.dropFirst()) : handle
	}

	var body: some View {
		let pieces = normalized.split(separator: "#", maxSplits: 1, omittingEmptySubsequences: false)
		let name = Text(verbatim: String(pieces[0])).foregroundStyle(.primary)
		let code = Text(verbatim: pieces.count > 1 ? "#" + pieces[1] : "").foregroundStyle(.secondary)
		Text("\(name)\(code)")
			.accessibilityLabel(Text(verbatim: normalized))
	}
}

struct IdentityText: View {
	let inboxID: String
	let handle: String?
	var short = false

	var body: some View {
		if let handle, !handle.isEmpty {
			HandleText(handle: handle)
		} else {
			Text(verbatim: String(inboxID.prefix(short ? 8 : 12)))
				.fontDesign(.monospaced)
				.accessibilityLabel(Text(verbatim: inboxID))
		}
	}
}
