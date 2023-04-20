use xmtp_proto::xmtp::message_api::v1::{
    cursor as cursor_proto, Cursor, Envelope, IndexCursor, PagingInfo,
    QueryResponse as ProtoQueryResponse, SortDirection,
};

impl From<crate::ffi::SortDirection> for SortDirection {
    fn from(sort_direction: crate::ffi::SortDirection) -> Self {
        match sort_direction {
            crate::ffi::SortDirection::Unspecified => SortDirection::Unspecified,
            crate::ffi::SortDirection::Ascending => SortDirection::Ascending,
            crate::ffi::SortDirection::Descending => SortDirection::Descending,
        }
    }
}

impl From<SortDirection> for crate::ffi::SortDirection {
    fn from(sort_direction: SortDirection) -> Self {
        match sort_direction {
            SortDirection::Unspecified => crate::ffi::SortDirection::Unspecified,
            SortDirection::Ascending => crate::ffi::SortDirection::Ascending,
            SortDirection::Descending => crate::ffi::SortDirection::Descending,
        }
    }
}

impl From<crate::ffi::PagingInfo> for PagingInfo {
    fn from(paging_info: crate::ffi::PagingInfo) -> Self {
        let cursor = match paging_info.cursor {
            Some(cursor) => Some(Cursor {
                cursor: Some(cursor_proto::Cursor::Index(IndexCursor {
                    digest: cursor.digest,
                    sender_time_ns: cursor.sender_time_ns,
                })),
            }),
            None => None,
        };

        PagingInfo {
            limit: paging_info.limit,
            direction: SortDirection::from(paging_info.direction).into(),
            cursor,
        }
    }
}

impl From<PagingInfo> for crate::ffi::PagingInfo {
    fn from(paging_info: PagingInfo) -> Self {
        let cursor = match paging_info.cursor {
            Some(cursor) => match cursor.cursor {
                Some(cursor_proto::Cursor::Index(index_cursor)) => Some(crate::ffi::IndexCursor {
                    digest: index_cursor.digest,
                    sender_time_ns: index_cursor.sender_time_ns,
                }),
                _ => None,
            },
            None => None,
        };
        
        crate::ffi::PagingInfo {
            limit: paging_info.limit,
            direction: crate::ffi::SortDirection::from(
                SortDirection::from_i32(paging_info.direction)
                    .unwrap_or(SortDirection::Unspecified),
            ),
            cursor,
        }
    }
}

impl From<crate::ffi::Envelope> for Envelope {
    fn from(envelope: crate::ffi::Envelope) -> Self {
        Envelope {
            content_topic: envelope.content_topic,
            timestamp_ns: envelope.timestamp_ns,
            message: envelope.message,
        }
    }
}

impl From<Envelope> for crate::ffi::Envelope {
    fn from(envelope: Envelope) -> Self {
        crate::ffi::Envelope {
            content_topic: envelope.content_topic,
            timestamp_ns: envelope.timestamp_ns,
            message: envelope.message,
        }
    }
}

pub struct QueryResponse {
    _envelopes: Vec<crate::ffi::Envelope>,
    _paging_info: Option<crate::ffi::PagingInfo>,
}

impl QueryResponse {
    pub fn envelopes(self) -> Vec<crate::ffi::Envelope> {
        self._envelopes
    }

    pub fn paging_info(self) -> Option<crate::ffi::PagingInfo> {
        self._paging_info
    }
}

impl From<ProtoQueryResponse> for QueryResponse {
    fn from(query_response: ProtoQueryResponse) -> Self {
        let envelopes = query_response
            .envelopes
            .into_iter()
            .map(crate::ffi::Envelope::from)
            .collect();

        let paging_info = query_response.paging_info.map(crate::ffi::PagingInfo::from);

        QueryResponse {
            _envelopes: envelopes,
            _paging_info: paging_info,
        }
    }
}
