import CryptoSwift
import Foundation

public extension String {
	var hexToData: Data {
		Data(hex: self)
	}
}
