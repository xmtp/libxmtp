//
//  Installation.swift
//
//
//  Created by Naomi Plasterer on 9/25/24.
//

import Foundation
import LibXMTP

public struct Installation {
	var ffiInstallation: FfiInstallation
    
    init(ffiInstallation: FfiInstallation) {
        self.ffiInstallation = ffiInstallation
    }

    public var id: String {
		ffiInstallation.id.toHex
    }
	
	public var createdAt: Date? {
		guard let timestampNs = ffiInstallation.clientTimestampNs else { return nil }
		return Date(timeIntervalSince1970: TimeInterval(timestampNs) / 1_000_000_000)
	}
}

