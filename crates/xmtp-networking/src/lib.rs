pub mod proto_helper;
use crate::proto_helper::xmtp::message_api::v1;

pub fn test_request() -> Result<u16, String> {
    let resp = reqwest::blocking::get("https://httpbin.org/ip").map_err(|e| format!("{}", e))?;
    // if resp is successful, return the body otherwise return "Error: {}" with response code
    if resp.status().is_success() {
        Ok(resp.status().as_u16())
    } else {
        Err(format!("{}", resp.status()))
    }
}

pub fn selftest() -> u16 {
    let resp = test_request();
    resp.unwrap_or(777)
}

pub async fn test_grpc() -> bool {
    let mut client =
        proto_helper::xmtp::message_api::v1::message_api_client::MessageApiClient::connect(
            "http://localhost:5556",
        )
        .await
        .unwrap();
    // TODO: Return true if client was able to connect
    true
}

// Do a barebones unpaginated Query gRPC request, similar to this Swift code:
// 	func query(topic: String, pagination: Pagination? = nil, cursor: Xmtp_MessageApi_V1_Cursor? = nil) async throws -> QueryResponse {
// 		var request = Xmtp_MessageApi_V1_QueryRequest()
// 		request.contentTopics = [topic]
// 
// 		if let pagination {
// 			request.pagingInfo = pagination.pagingInfo
// 		}
// 
// 		if let startAt = pagination?.startTime {
// 			request.endTimeNs = UInt64(startAt.millisecondsSinceEpoch) * 1_000_000
// 			request.pagingInfo.direction = .descending
// 		}
// 
// 		if let endAt = pagination?.endTime {
// 			request.startTimeNs = UInt64(endAt.millisecondsSinceEpoch) * 1_000_000
// 			request.pagingInfo.direction = .descending
// 		}
// 
// 		if let cursor {
// 			request.pagingInfo.cursor = cursor
// 		}
// 
// 		var options = CallOptions()
// 		options.customMetadata.add(name: "authorization", value: "Bearer \(authToken)")
// 		options.timeLimit = .timeout(.seconds(5))
// 
// 		return try await client.query(request, callOptions: options)
// 	}
pub async fn query(
    topic: String,
) -> Result<v1::QueryResponse, tonic::Status> {
    let mut client =
        proto_helper::xmtp::message_api::v1::message_api_client::MessageApiClient::connect(
            "http://dev.xmtp.network:5556",
        )
        .await
        .unwrap();

    let mut request = proto_helper::xmtp::message_api::v1::QueryRequest::default();
    request.content_topics = vec![topic];
    // Do the query and get a Tonic response that we need to process
    let response = client.query(request).await;
    response.map(|r| r.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_works() {
        let resp = selftest();
        // Assert 200
        assert_eq!(resp, 200);
    }

    #[test]
    fn grpc_query_test() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let resp = query("test".to_string()).await;
            println!("{:?}", resp);
            assert!(resp.is_ok());
        });
    }
}
