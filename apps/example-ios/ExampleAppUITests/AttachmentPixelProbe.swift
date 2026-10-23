import UIKit
import XCTest

@MainActor enum AttachmentPixelProbe {
	static func matches(_ element: XCUIElement, in app: XCUIApplication) -> Bool {
		guard element.exists, let screenshot = app.screenshot().image.cgImage else { return false }
		let frame = element.frame
		guard frame.width > 40, frame.height > 40,
		      let teal = pixel(screenshot, at: CGPoint(x: frame.minX + frame.width / 8, y: frame.midY), screen: app.frame),
		      let yellow = pixel(screenshot, at: CGPoint(x: frame.midX, y: frame.midY), screen: app.frame)
		else { return false }
		return teal[0] < 60 && teal[1] > 120 && teal[2] > 120
			&& yellow[0] > 190 && yellow[1] > 140 && yellow[2] < 80
	}

	private static func pixel(_ image: CGImage, at point: CGPoint, screen: CGRect) -> [UInt8]? {
		let scaleX = CGFloat(image.width) / screen.width, scaleY = CGFloat(image.height) / screen.height
		let rect = CGRect(x: (point.x - screen.minX) * scaleX, y: (point.y - screen.minY) * scaleY, width: 1, height: 1)
		guard let sample = image.cropping(to: rect),
		      let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
		var bytes = [UInt8](repeating: 0, count: 4)
		let drawn = bytes.withUnsafeMutableBytes { buffer -> Bool in
			guard let context = CGContext(
				data: buffer.baseAddress,
				width: 1,
				height: 1,
				bitsPerComponent: 8,
				bytesPerRow: 4,
				space: colorSpace,
				bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue,
			) else { return false }
			context.draw(sample, in: CGRect(x: 0, y: 0, width: 1, height: 1))
			return true
		}
		return drawn ? bytes : nil
	}
}
