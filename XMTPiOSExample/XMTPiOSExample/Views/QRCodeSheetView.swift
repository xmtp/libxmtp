//
//  QRCodeSheetView.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 11/22/22.
//

import SwiftUI

struct QRCodeSheetView: View {
	var image: UIImage

	var body: some View {
		Image(uiImage: image)
	}
}
