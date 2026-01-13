//! Action handlers for network and node operations.
//!
//! This module contains reusable action logic that can be called from views.
//! Actions are async operations that update the application state.

use crate::state::{NetworkStatus, NodeInfo, ServiceInfo, ToxicInfo};
use color_eyre::eyre::Result;
use tokio::sync::mpsc;
use xmtp_api_d14n::d14n::GetNodes;
use xmtp_api_grpc::GrpcClient;
use xmtp_proto::api::Query;
use xnet::app::App;
use xnet::config::AddNode;

/// Check if Docker is installed and the daemon is responding.
pub async fn check_docker() -> Result<()> {
    xnet::network::check_docker_available().await
}

/// Check if DNS is configured correctly for *.xmtpd.local.
pub async fn check_dns() -> Result<()> {
    xnet::dns_setup::check_dns_configured().await
}

/// Creates a ToxiProxy client from config.
fn make_toxi_client() -> Result<toxiproxy_rust::client::Client> {
    let config = xnet::Config::load()?;
    let api_port: u16 = if config.use_standard_ports {
        8474
    } else {
        8555
    };
    Ok(toxiproxy_rust::client::Client::new(format!(
        "127.0.0.1:{}",
        api_port
    )))
}

/// Starts a background tokio task that polls the ToxiProxy API for registered
/// proxies every 2 seconds. Returns a receiver that yields updated service lists.
pub async fn start_service_poller() -> Result<mpsc::Receiver<Vec<ServiceInfo>>> {
    let (tx, rx) = mpsc::channel(4);
    let client = make_toxi_client()?;

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            match client.all().await {
                Ok(proxies) => {
                    let mut services: Vec<ServiceInfo> = proxies
                        .into_iter()
                        .map(|(name, proxy)| {
                            let port = proxy
                                .proxy_pack
                                .listen
                                .rsplit(':')
                                .next()
                                .and_then(|p| p.parse::<u16>().ok());
                            let url = port.map(|p| format!("http://localhost:{}", p));
                            let status = if proxy.proxy_pack.enabled {
                                "running"
                            } else {
                                "disabled"
                            };
                            ServiceInfo {
                                name,
                                status,
                                external_url: url,
                            }
                        })
                        .collect();
                    services.sort_by(|a, b| a.name.cmp(&b.name));
                    // Add non-ToxiProxy services with fixed localhost ports
                    services.push(ServiceInfo {
                        name: "otterscan".into(),
                        status: "running",
                        external_url: Some("http://localhost:5100".into()),
                    });
                    services.push(ServiceInfo {
                        name: "traefik".into(),
                        status: "running",
                        external_url: Some("http://localhost:8080".into()),
                    });
                    if tx.send(services).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("ToxiProxy poll failed: {e}");
                }
            }
        }
    });

    Ok(rx)
}

pub async fn execute_up() -> Result<()> {
    let _ = xnet::Config::load()?;
    tracing::info!("up");
    App::parse()?.up().await?;
    Ok(())
}

pub async fn execute_down() -> Result<()> {
    App::parse()?.down().await?;
    Ok(())
}

pub async fn execute_delete() -> Result<()> {
    App::parse()?.delete().await?;
    Ok(())
}

pub async fn execute_add_node() -> Result<NodeInfo> {
    let node = App::parse()?.add_node(&AddNode { migrator: false }).await?;
    Ok(NodeInfo {
        id: *node.id(),
        container_name: format!("xnet-{}", node.id()),
        url: format!("http://node{}.xmtpd.local", node.id()),
    })
}

pub async fn execute_add_migrator() -> Result<NodeInfo> {
    let node = App::parse()?.add_node(&AddNode { migrator: true }).await?;
    Ok(NodeInfo {
        id: *node.id(),
        container_name: format!("xnet-{}", node.id()),
        url: format!("http://node{}.xmtpd.local", node.id()),
    })
}

/// Helper to check if an action can be executed based on current state.
pub fn can_add_node(status: NetworkStatus) -> bool {
    status == NetworkStatus::Running
}

// -- Toxics Management --------------------------------------------------------

/// Starts a background tokio task that polls toxics from all proxies every 2
/// seconds. Returns a receiver that yields updated toxic lists.
pub async fn start_toxics_poller() -> Result<mpsc::Receiver<Vec<ToxicInfo>>> {
    let (tx, rx) = mpsc::channel(4);
    let client = make_toxi_client()?;

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            match client.all().await {
                Ok(proxies) => {
                    let mut all_toxics = Vec::new();
                    for (name, proxy) in &proxies {
                        match proxy.toxics().await {
                            Ok(toxics) => {
                                for t in toxics {
                                    all_toxics.push(ToxicInfo {
                                        proxy_name: name.clone(),
                                        toxic_type: t.r#type.clone(),
                                        stream: t.stream.clone(),
                                        toxicity: t.toxicity,
                                        latency: t.attributes.get("latency").copied(),
                                        jitter: t.attributes.get("jitter").copied(),
                                    });
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to get toxics for {}: {}", name, e);
                            }
                        }
                    }
                    all_toxics.sort_by(|a, b| a.proxy_name.cmp(&b.proxy_name));
                    if tx.send(all_toxics).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("Toxics poller failed: {e}");
                }
            }
        }
    });

    Ok(rx)
}

/// Add a latency toxic to the named proxy (downstream, 100% toxicity).
pub async fn add_latency_toxic(proxy_name: String, latency_ms: u32) -> Result<()> {
    let client = make_toxi_client()?;
    let proxy = client
        .find_proxy(&proxy_name)
        .await
        .map_err(|e| color_eyre::eyre::eyre!(e))?;
    proxy
        .with_latency("downstream".to_string(), latency_ms, 0, 1.0)
        .await;
    Ok(())
}

/// Delete all toxics from a specific proxy and re-enable it.
pub async fn reset_proxy_toxics(proxy_name: String) -> Result<()> {
    let client = make_toxi_client()?;
    client
        .find_and_reset_proxy(&proxy_name)
        .await
        .map_err(|e| color_eyre::eyre::eyre!(e))?;
    Ok(())
}

/// Reset all proxies — remove all toxics and re-enable everything.
pub async fn reset_all_toxics() -> Result<()> {
    let client = make_toxi_client()?;
    client
        .reset()
        .await
        .map_err(|e| color_eyre::eyre::eyre!(e))?;
    Ok(())
}

/// Starts a background tokio task that polls the gateway's GetNodes endpoint
/// every 2 seconds. Returns a receiver that yields updated node lists.
pub async fn start_node_poller() -> Result<mpsc::Receiver<Vec<NodeInfo>>> {
    let (tx, rx) = mpsc::channel(4);

    let gateway_url = App::parse()?.gateway_url().await?;
    let grpc = GrpcClient::create(gateway_url.as_str(), false)?;

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            match GetNodes::builder().build().unwrap().query(&grpc).await {
                Ok(response) => {
                    let mut infos: Vec<NodeInfo> = response
                        .nodes
                        .into_iter()
                        .map(|(id, url)| NodeInfo {
                            id,
                            container_name: format!("xnet-{}", id),
                            url,
                        })
                        .collect();
                    infos.sort_by_key(|n| n.id);
                    if tx.send(infos).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("GetNodes poll failed: {e}");
                }
            }
        }
    });

    Ok(rx)
}
