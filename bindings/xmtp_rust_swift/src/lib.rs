#[swift_bridge::bridge]
mod ffi {
    #[swift_bridge(swift_repr = "struct")]
    struct ResponseJson {
        error: String,
        json: String
    }

    extern "Rust" {
        async fn query_topic(topic: String) -> ResponseJson;
    }
}

// TODO: Return a `Result<MyIpAddress, SomeErrorType>`
//  Once we support returning Result from an async function.
async fn query_topic(topic: String) -> ffi::ResponseJson {
    println!("Received a request to query topic: {}", topic);
    let query_result = xmtp_networking::query_serialized(topic).await;
    match query_result {
        Ok(json) => ffi::ResponseJson {
            error: "".to_string(),
            json,
        },
        Err(e) => ffi::ResponseJson {
            error: e.to_string(),
            json: "".to_string(),
        },
    }
}

#[no_mangle]
pub extern "C" fn grpc_selftest() -> u16 {
    // Returns 0 if successful, >0 if failed
    xmtp_networking::grpc_roundtrip()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_networking() {
        let status_code = networking_selftest();
        assert_eq!(status_code, 200);
    }
}
