import ExampleCore
import SwiftUI
import UIKit

private struct AttachmentPreviews: View {
	private let image = AttachmentPreviewImage.make()
	var body: some View {
		ScrollView {
			VStack(alignment: .leading, spacing: 20) {
				AttachmentSurface(preview: image, isImage: true, name: "Mountain lake.jpg", type: "JPG",
				                  bytes: 2_400_000, phase: .verified, own: false, showsMetadata: false)
				{
					Text("Alex · 10:42 AM")
				}
				AttachmentSurface(preview: image, isImage: true, name: "Mountain lake.jpg", type: "JPG",
				                  bytes: 2_400_000, phase: .verified, own: false, showsMetadata: true)
				{
					Text("Alex · 10:42 AM").font(.caption)
				}
				AttachmentSurface(preview: nil, isImage: false, name: "Weekend itinerary.pdf", type: "PDF",
				                  bytes: 840_000, phase: .verified, own: true, showsMetadata: true)
				{
					Text("You · 10:43 AM").font(.caption)
				}
				AttachmentSurface(
					preview: nil,
					isImage: false,
					name: "A longer document filename for small screens.pdf",
					type: "PDF",
					bytes: 840_000,
					phase: .downloading,
					own: false,
					showsMetadata: false,
				) { EmptyView() }
				AttachmentSurface(preview: nil, isImage: true, name: "Photo.jpg", type: "JPG", bytes: nil,
				                  phase: .failed, own: false, showsMetadata: false) { EmptyView() }
			}.padding(16)
		}.background(Color(uiColor: .systemBackground))
	}
}

@MainActor private enum AttachmentPreviewImage {
	static func make() -> CGImage? {
		UIGraphicsImageRenderer(size: CGSize(width: 640, height: 520)).image { context in
			let canvas = context.cgContext
			canvas.setFillColor(UIColor(red: 0.73, green: 0.84, blue: 0.85, alpha: 1).cgColor)
			canvas.fill(CGRect(x: 0, y: 0, width: 640, height: 520))
			canvas.setFillColor(UIColor(red: 0.39, green: 0.56, blue: 0.56, alpha: 1).cgColor)
			canvas.move(to: CGPoint(x: 0, y: 300))
			canvas.addLines(between: [CGPoint(x: 180, y: 45), CGPoint(x: 300, y: 220),
			                          CGPoint(x: 410, y: 60), CGPoint(x: 640, y: 300)])
			canvas.closePath(); canvas.fillPath()
			canvas.setFillColor(UIColor(red: 0.16, green: 0.4, blue: 0.44, alpha: 1).cgColor)
			canvas.fill(CGRect(x: 0, y: 300, width: 640, height: 220))
		}.cgImage
	}
}

#Preview("Attachments · Light") { AttachmentPreviews().preferredColorScheme(.light) }
#Preview("Attachments · Dark") { AttachmentPreviews().preferredColorScheme(.dark) }
#Preview("Attachments · AX3") { AttachmentPreviews().dynamicTypeSize(.accessibility3) }
