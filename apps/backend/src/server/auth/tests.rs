use super::*;
use crate::{
    auth::{
        AuthContext,
        keys::{KeySet, inline},
    },
    config::auth::AuthConfig,
    test_support::auth::{TestKey, mint, valid_claims},
};
use http_body_util::BodyExt;
use std::convert::Infallible;
use tower::{ServiceExt, service_fn};

mod redaction;
mod streams;
mod transport;

fn verifier(config: AuthConfig) -> Arc<Verifier> {
    Arc::new(Verifier::new(
        Arc::new(KeySet::new(inline(&config).unwrap())),
        config,
    ))
}

#[xmtp_common::test(unwrap_try = true)]
async fn auth_disabled_does_not_read_headers_or_insert_context() {
    let layer = tower::ServiceBuilder::new().option_layer(None::<AuthLayer>);
    let service = layer.service(service_fn(|request: Request<Body>| async move {
        assert!(request.extensions().get::<AuthContext>().is_none());
        assert_eq!(request.headers()["authorization"].as_bytes(), &[0xff]);
        Ok::<_, Infallible>(Response::new(request.into_body()))
    }));
    let mut request = Request::post("/xmtp.backend.v1.QueryService/Query").body(Body::empty())?;
    request
        .headers_mut()
        .insert("authorization", http::HeaderValue::from_bytes(&[0xff])?);
    service.oneshot(request).await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn success_preserves_request_body_headers_and_extensions() {
    let key = TestKey::es256();
    let config = key.auth_config();
    let mut claims = valid_claims();
    claims["sub"] = serde_json::json!("caller");
    claims["scope"] = serde_json::json!("xmtp publish");
    let authorization = format!("Bearer {}", mint(&claims, &key));
    let expected = authorization.clone();
    let service = AuthLayer(verifier(config)).layer(service_fn(move |request: Request<Body>| {
        let expected = expected.clone();
        async move {
            assert_eq!(request.uri().path(), "/xmtp.backend.v1.QueryService/Query");
            assert_eq!(request.headers()["authorization"], expected);
            assert_eq!(request.extensions().get::<u32>(), Some(&42));
            let context = request.extensions().get::<AuthContext>().unwrap();
            assert_eq!(context.sub.as_deref(), Some("caller"));
            assert_eq!(
                context.scopes,
                std::collections::BTreeSet::from(["xmtp".into(), "publish".into()])
            );
            Ok::<_, Infallible>(Response::new(request.into_body()))
        }
    }));
    let mut request = Request::post("/xmtp.backend.v1.QueryService/Query")
        .header("authorization", authorization)
        .body(Body::new(http_body_util::Full::new(
            bytes::Bytes::from_static(b"unchanged"),
        )))?;
    request.extensions_mut().insert(42_u32);
    let response = service.oneshot(request).await?;
    assert_eq!(
        response.into_body().collect().await?.to_bytes(),
        "unchanged"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn path_gate_ignores_method_and_content_type_and_exempts_only_health() {
    let verifier = verifier(TestKey::es256().auth_config());
    for path in ["/xmtp.backend.v1.QueryService/Query", "/unknown", "/"] {
        for method in [http::Method::POST, http::Method::GET] {
            let service =
                AuthLayer(verifier.clone()).layer(service_fn(|_: Request<Body>| async move {
                    Ok::<_, Infallible>(
                        Response::builder()
                            .header("handler-reached", "true")
                            .body(Body::empty())
                            .unwrap(),
                    )
                }));
            let response = service
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .method(method)
                        .header("content-type", "text/plain")
                        .body(Body::empty())?,
                )
                .await?;
            assert_eq!(response.headers()["grpc-status"], "16");
            assert!(!response.headers().contains_key("handler-reached"));
        }
    }
    let service = AuthLayer(verifier).layer(service_fn(|request: Request<Body>| async move {
        assert!(request.extensions().get::<AuthContext>().is_none());
        Ok::<_, Infallible>(Response::new(Body::empty()))
    }));
    let response = service
        .oneshot(Request::post("/grpc.health.v1.Health/Check").body(Body::empty())?)
        .await?;
    assert!(!response.headers().contains_key("grpc-status"));
}

#[xmtp_common::test(unwrap_try = true)]
async fn required_scopes_apply_to_every_path_including_unknown_ones() {
    let key = TestKey::es256();
    let mut config = key.auth_config();
    config.required_scopes = vec!["xmtp".into(), "publish".into()];
    let verifier = verifier(config);
    // Scopes are global, so the same token decides every path the same way.
    // 7 is PERMISSION_DENIED, 0 is OK.
    for (path, scope, expected) in [
        ("/xmtp.backend.v1.PublishService/Publish", "xmtp", "7"),
        (
            "/xmtp.backend.v1.PublishService/Publish",
            "xmtp publish",
            "0",
        ),
        ("/xmtp.backend.v1.QueryService/Query", "xmtp publish", "0"),
        ("/unknown", "xmtp", "7"),
        ("/unknown", "xmtp publish", "0"),
    ] {
        let mut claims = valid_claims();
        claims["scope"] = serde_json::json!(scope);
        let service = AuthLayer(verifier.clone()).layer(service_fn(|_: Request<Body>| async {
            Ok::<_, Infallible>(tonic::Status::ok("").into_http())
        }));
        let response = service
            .oneshot(
                Request::post(path)
                    .header("authorization", format!("Bearer {}", mint(&claims, &key)))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.headers()["grpc-status"], expected);
    }
}
