//! Traefik dynamic configuration manager.
//!
//! Manages the dynamic.yml file that Traefik watches for routing updates.
//! Thread-safe for concurrent access from multiple services.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use color_eyre::eyre::{Result, eyre};
use serde::{Deserialize, Serialize};

/// Traefik dynamic configuration manager.
///
/// This struct manages the dynamic.yml file that Traefik watches.
/// Routes map hostnames to ToxiProxy ports for unified addressing.
#[derive(Clone, Debug)]
pub struct TraefikConfig {
    /// Path to the dynamic configuration file
    config_path: PathBuf,
    /// Hostname -> ToxiProxy port mapping (thread-safe)
    routes: Arc<Mutex<HashMap<String, u16>>>,
}

/// Traefik dynamic configuration structure (for YAML serialization)
#[derive(Debug, Serialize, Deserialize)]
struct TraefikDynamicConfig {
    http: HttpConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct ServersTransport {
    #[serde(rename = "insecureSkipVerify", skip_serializing_if = "Option::is_none")]
    insecure_skip_verify: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HttpConfig {
    routers: HashMap<String, Router>,
    services: HashMap<String, TraefikService>,
    #[serde(rename = "serversTransports", skip_serializing_if = "Option::is_none")]
    servers_transports: Option<HashMap<String, ServersTransport>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Router {
    rule: String,
    service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TraefikService {
    #[serde(rename = "loadBalancer")]
    load_balancer: LoadBalancer,
}

#[derive(Debug, Serialize, Deserialize)]
struct LoadBalancer {
    servers: Vec<Server>,
    #[serde(rename = "serversTransport", skip_serializing_if = "Option::is_none")]
    servers_transport: Option<String>,
    #[serde(rename = "passHostHeader", skip_serializing_if = "Option::is_none")]
    pass_host_header: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Server {
    url: String,
}

impl TraefikConfig {
    /// Load existing routes from the dynamic.yml file.
    ///
    /// Parses the YAML file and extracts hostname -> port mappings.
    fn load_from_file(config_path: &Path) -> HashMap<String, u16> {
        // Return empty if file doesn't exist
        if !config_path.exists() {
            return HashMap::new();
        }

        // Read file contents
        let contents = match fs::read_to_string(config_path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        // Parse YAML
        let config: TraefikDynamicConfig = match serde_yaml::from_str(&contents) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        let mut routes = HashMap::new();

        // Extract routes from routers and services
        for (name, router) in config.http.routers {
            // Extract hostname from "Host(`node0.xmtpd.local`)"
            if let Some(hostname) = router
                .rule
                .strip_prefix("Host(`")
                .and_then(|s| s.strip_suffix("`)"))
            {
                // Get port from corresponding service
                if let Some(service) = config.http.services.get(&router.service)
                    && let Some(server) = service.load_balancer.servers.first()
                {
                    // Parse "http://toxiproxy:8150" -> 8150
                    if let Some((_, port_str)) = server.url.rsplit_once(':')
                        && let Ok(port) = port_str.parse()
                    {
                        routes.insert(hostname.to_string(), port);
                    }
                }
            }
        }

        routes
    }

    /// Create a new Traefik config manager.
    ///
    /// The config file will be written to the specified path.
    /// Creates parent directories if they don't exist.
    /// Loads existing routes from the file if it exists.
    pub fn new(config_path: impl Into<PathBuf>) -> Result<Self> {
        let config_path = config_path.into();

        // Create parent directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Load existing routes from file
        let routes = Self::load_from_file(&config_path);
        info!(
            "Loaded {} existing routes from {}",
            routes.len(),
            config_path.display()
        );

        Ok(Self {
            config_path,
            routes: Arc::new(Mutex::new(routes)),
        })
    }

    /// Add a route mapping hostname to ToxiProxy port.
    ///
    /// This will update the dynamic.yml file, which Traefik will automatically reload.
    pub fn add_route(&self, hostname: impl Into<String>, toxi_port: u16) -> Result<()> {
        let hostname = hostname.into();

        {
            let mut routes = self.routes.lock().unwrap();
            routes.insert(hostname.clone(), toxi_port);
        }

        self.write()?;
        info!(
            "Added Traefik route: {} -> toxiproxy:{}",
            hostname, toxi_port
        );
        Ok(())
    }

    /// Remove a route for the given hostname.
    ///
    /// This will update the dynamic.yml file, which Traefik will automatically reload.
    pub fn remove_route(&self, hostname: &str) -> Result<()> {
        {
            let mut routes = self.routes.lock().unwrap();
            routes.remove(hostname);
        }

        self.write()?;
        info!("Removed Traefik route: {}", hostname);
        Ok(())
    }

    /// Get all current routes.
    pub fn routes(&self) -> HashMap<String, u16> {
        self.routes.lock().unwrap().clone()
    }

    /// Write the current configuration to the dynamic.yml file.
    ///
    /// Traefik will automatically detect and reload the changes.
    fn write(&self) -> Result<()> {
        let routes = self.routes.lock().unwrap();

        let mut routers = HashMap::new();
        let mut services = HashMap::new();

        for (hostname, toxi_port) in routes.iter() {
            // Generate a safe router/service name from hostname
            let name = hostname.replace(['.', '-'], "_");

            routers.insert(
                name.clone(),
                Router {
                    rule: format!("Host(`{}`)", hostname),
                    service: name.clone(),
                    priority: None,
                },
            );

            services.insert(
                name,
                TraefikService {
                    load_balancer: LoadBalancer {
                        servers: vec![Server {
                            url: format!("h2c://xnet-toxiproxy:{}", toxi_port),
                        }],
                        servers_transport: None,
                        pass_host_header: Some(true),
                    },
                },
            );
        }

        let config = TraefikDynamicConfig {
            http: HttpConfig {
                routers,
                services,
                servers_transports: None,
            },
        };

        let yaml = serde_yaml::to_string(&config)?;
        fs::write(&self.config_path, yaml)?;

        debug!(
            "Wrote Traefik config with {} routes to {}",
            routes.len(),
            self.config_path.display()
        );

        Ok(())
    }

    /// Get the config file path.
    pub fn path(&self) -> &Path {
        &self.config_path
    }

    /// Print /etc/hosts entries for all registered routes.
    ///
    /// Returns a formatted string that can be appended to /etc/hosts.
    /// All hostnames resolve to 127.0.0.1 for local access through Traefik.
    pub fn hosts_entries(&self) -> String {
        let routes = self.routes.lock().unwrap();

        if routes.is_empty() {
            return String::new();
        }

        let mut entries = vec!["# xnet hostname entries (generated by xnet)".to_string()];

        for hostname in routes.keys() {
            entries.push(format!("127.0.0.1 {}", hostname));
        }

        entries.join("\n")
    }
}
