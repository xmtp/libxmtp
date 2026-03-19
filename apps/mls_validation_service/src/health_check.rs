use std::future::Future;

use warp::Filter;

use crate::wait_for_quit;

pub async fn health_check_server(port: u16) -> impl Future<Output = ()> {
    let health_route =
        warp::path("health").map(|| warp::reply::with_status("ok", warp::http::StatusCode::OK));

    warp::serve(health_route)
        .bind(([0, 0, 0, 0], port))
        .await
        .graceful(async {
            wait_for_quit().await;
            info!("HTTP server shutdown signal received");
        })
        .run()
}
