//
//  PagingInfo.swift
//
//
//  Created by Pat Nakajima on 12/15/22.
//

import Foundation

typealias PagingInfo = Xmtp_MessageApi_V1_PagingInfo
typealias PagingInfoCursor = Xmtp_MessageApi_V1_Cursor
typealias PagingInfoSortDirection = Xmtp_MessageApi_V1_SortDirection

public struct Pagination {
	public var limit: Int?
	var direction: PagingInfoSortDirection?
    public var before: Date?
    public var after: Date?

	var pagingInfo: PagingInfo {
		var info = PagingInfo()

		if let limit {
			info.limit = UInt32(limit)
		}
        info.direction = direction ?? Xmtp_MessageApi_V1_SortDirection.descending
		return info
	}
}

extension PagingInfo {
	init(limit: Int? = nil, cursor: PagingInfoCursor? = nil, direction: PagingInfoSortDirection? = nil) {
		self.init()

		if let limit {
			self.limit = UInt32(limit)
		}

		if let cursor {
			self.cursor = cursor
		}

		if let direction {
			self.direction = direction
		}
	}
}
