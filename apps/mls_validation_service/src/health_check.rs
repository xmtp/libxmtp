use std::future::Future;

use warp::Filter;

use crate::wait_for_quit;

pub fn health_check_server(port: u16) -> impl Future<Output = ()> {
    let health_route =
        warp::path("health").map(|| warp::reply::with_status("ok", warp::http::StatusCode::OK));

    let (_, health_server) =
        warp::serve(health_route).bind_with_graceful_shutdown(([0, 0, 0, 0], port), async {
            wait_for_quit().await;
            info!("HTTP server shutdown signal received");
        });

    health_server
}
