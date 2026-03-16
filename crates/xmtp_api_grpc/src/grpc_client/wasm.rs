use crate::error::GrpcBuilderError;
use http::Request;
use std::task::{Context, Poll};
use tonic::{body::Body, client::GrpcService};
use tonic_web_wasm_client::{
    Client,
    options::{FetchOptions, ReferrerPolicy},
};
use tower::Service;

#[derive(Debug, Clone)]
pub struct GrpcWebService {
    inner: Client,
}

impl GrpcWebService {
    pub fn new(host: url::Url, _limit: Option<u64>) -> Result<Self, GrpcBuilderError> {
        // envoy does _not_ like trailing /
        let url = host.as_str().trim_end_matches("/");
        let options =
            FetchOptions::default().referrer_policy(ReferrerPolicy::StrictOriginWhenCrossOrigin);
        Ok(Self {
            inner: Client::new_with_options(url.into(), options),
        })
    }
}

impl Service<Request<Body>> for GrpcWebService {
    type Response = <Client as Service<Request<Body>>>::Response;
    type Error = <Client as GrpcService<Body>>::Error;
    type Future = <Client as GrpcService<Body>>::Future;

    fn poll_ready(&mut self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        <Client as Service<Request<Body>>>::poll_ready(&mut self.inner, ctx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        <Client as Service<Request<Body>>>::call(&mut self.inner, request)
    }
}
