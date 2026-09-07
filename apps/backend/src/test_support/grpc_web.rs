use super::TestResult;
use futures::StreamExt;
use http_body_util::{BodyExt, StreamBody};
use prost::Message;
use tonic::body::Body;
use tower::{Layer, service_fn};

pub struct WebStream<T> {
    pub headers: reqwest::header::HeaderMap,
    pub stream: tonic::Streaming<T>,
}

/// Use Tonic's public gRPC-Web client layer over real HTTP/1.1. Only the finite
/// request is collected; response frames and trailers remain streamed. The
/// supplied client's default headers and TLS configuration are preserved.
pub async fn open<Q, R>(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    request: Q,
) -> TestResult<WebStream<R>>
where
    Q: Message + Send + 'static,
    R: Message + Default + Send + 'static,
{
    let client = client.clone();
    let transport = service_fn(
        move |request: http::Request<tonic_web::GrpcWebCall<Body>>| {
            let client = client.clone();
            async move {
                let (parts, body) = request.into_parts();
                let body = BodyExt::collect(body).await?.to_bytes();
                let response = client
                    .request(parts.method, parts.uri.to_string())
                    .version(parts.version)
                    .headers(parts.headers)
                    .body(body)
                    .send()
                    .await?;
                let status = response.status();
                let headers = response.headers().clone();
                let body = StreamBody::new(
                    response
                        .bytes_stream()
                        .map(|bytes| bytes.map(http_body::Frame::data)),
                );
                let mut response = http::Response::new(Body::new(body));
                *response.status_mut() = status;
                *response.headers_mut() = headers;
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(response)
            }
        },
    );
    let transport = tonic_web::GrpcWebClientLayer::new().layer(transport);
    let mut grpc = tonic::client::Grpc::with_origin(transport, url.parse()?);
    grpc.ready().await?;
    let response = grpc
        .server_streaming(
            tonic::Request::new(request),
            method.parse()?,
            tonic_prost::ProstCodec::<Q, R>::default(),
        )
        .await?;
    let headers = response.metadata().clone().into_headers();
    Ok(WebStream {
        headers,
        stream: response.into_inner(),
    })
}
