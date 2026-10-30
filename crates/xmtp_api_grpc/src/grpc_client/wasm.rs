use crate::error::GrpcBuilderError;
use http::Request;
use std::task::{Context, Poll};
use tonic::{body::Body, client::GrpcService};
use tonic_web_wasm_client::{
    Client,
    options::{FetchOptions, Redirect, ReferrerPolicy},
};
use tower::Service;

#[derive(Debug, Clone)]
pub struct GrpcWebService {
    inner: Client,
}

impl GrpcWebService {
    pub fn new(host: url::Url, _limit: Option<u64>) -> Result<Self, GrpcBuilderError> {
        // Remove the trailing slash before adding the RPC path.
        let url = host.as_str().trim_end_matches("/");
        // implements: CONF-096
        let options = FetchOptions::default()
            .redirect(Redirect::Error)
            .referrer_policy(ReferrerPolicy::StrictOriginWhenCrossOrigin);
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

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen::prelude::*;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen(inline_js = r#"
        let savedFetch;
        let redirects;
        export function install_fetch_probe() {
            savedFetch = globalThis.fetch;
            redirects = [];
            globalThis.fetch = (input, init) => {
                redirects.push(new Request(input, init).redirect);
                return Promise.reject(new Error("fetch probe sentinel"));
            };
        }
        export function fetch_probe_calls() { return redirects.length; }
        export function restore_fetch_probe() {
            globalThis.fetch = savedFetch;
            return redirects.join(",");
        }
    "#)]
    extern "C" {
        fn install_fetch_probe();
        fn fetch_probe_calls() -> u32;
        fn restore_fetch_probe() -> String;
    }

    // verifies: CONF-096
    #[xmtp_common::test(unwrap_try = true)]
    async fn fetch_uses_error_redirect_mode() {
        let mut service = GrpcWebService::new("https://localhost".parse()?, None)?;
        let request = Request::builder()
            .uri("/test.Service/Call")
            .body(Body::empty())?;
        install_fetch_probe();
        let result = Service::call(&mut service, request).await;
        let calls = fetch_probe_calls();
        let redirect = restore_fetch_probe();
        assert!(result.is_err());
        assert_eq!(calls, 1);
        assert_eq!(redirect, "error");
    }
}
