use xmtp_networking::grpc_api_helper;

#[swift_bridge::bridge]
mod ffi {
    #[swift_bridge(swift_repr = "struct")]
    struct ResponseJson {
        error: String,
        json: String,
    }

    extern "Rust" {
        async fn query(host: String, topic: String, json_paging_info: String) -> ResponseJson;
        async fn publish(host: String, token: String, json_envelopes: String) -> ResponseJson;
    }
}

async fn query(host: String, topic: String, json_paging_info: String) -> ffi::ResponseJson {
    println!(
        "Received a request to query host: {}, topic: {}, paging info: {}",
        host, topic, json_paging_info
    );
    let query_result = grpc_api_helper::query_serialized(host, topic, json_paging_info).await;
    match query_result {
        Ok(json) => ffi::ResponseJson {
            error: "".to_string(),
            json,
        },
        Err(e) => ffi::ResponseJson {
            error: e,
            json: "".to_string(),
        },
    }
}

async fn publish(host: String, token: String, json_envelopes: String) -> ffi::ResponseJson {
    println!(
        "Received a request to publish host: {}, token: {}, envelopes: {}",
        host, token, json_envelopes
    );
    let publish_result = grpc_api_helper::publish_serialized(host, token, json_envelopes).await;
    match publish_result {
        Ok(json) => ffi::ResponseJson {
            error: "".to_string(),
            json,
        },
        Err(e) => ffi::ResponseJson {
            error: e,
            json: "".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    // Try a query on a test topic, and make sure we get a response
    #[test]
    fn test_query() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime.block_on(super::query(
            "http://localhost:5556".to_string(),
            "test".to_string(),
            "".to_string(),
        ));
        assert_eq!(result.error, "");
        println!("Got result: {}", result.json);
    }
}
