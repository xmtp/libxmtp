mod handlers;
mod validation_helpers;

use handlers::ValidationServer;
use tonic::transport::Server;
use xmtp_proto::xmtp::mls_validation::v1::validation_api_server::ValidationApiServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;

    Server::builder()
        .add_service(ValidationApiServer::new(ValidationServer::default()))
        .serve(addr)
        .await?;

    Ok(())
}
