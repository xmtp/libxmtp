import Foundation
import CryptoSwift

extension String {
	public var hexToData: Data {
		return Data(hex: self)
	}
}

