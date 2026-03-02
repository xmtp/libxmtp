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
    pub fn new(
        host: String,
        _limit: Option<u64>,
        _is_secure: bool,
    ) -> Result<Self, GrpcBuilderError> {
        let options =
            FetchOptions::default().referrer_policy(ReferrerPolicy::StrictOriginWhenCrossOrigin);
        Ok(Self {
            inner: Client::new_with_options(host, options),
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
