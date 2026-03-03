use thiserror::Error;
use xmtp_common::ErrorCode;
use xmtp_configuration::XmtpEnv;

#[derive(Debug, Clone)]
pub struct ResolvedBackendConfig {
    pub api_url: Option<String>,
    pub gateway_host: Option<String>,
    pub is_secure: bool,
    pub readonly: bool,
    pub app_version: String,
}

#[derive(Error, Debug, ErrorCode)]
pub enum BackendConfigError {
    #[error("D14n environment '{0:?}' requires gateway_host to be set")]
    MissingGatewayHost(XmtpEnv),
    #[error("Authentication (auth_callback or auth_handle) requires gateway_host to be set")]
    AuthRequiresGateway,
}

fn is_url_secure(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("grpcs://")
}

pub fn validate_and_resolve(
    env: XmtpEnv,
    api_url_override: Option<String>,
    gateway_host: Option<String>,
    readonly: bool,
    app_version: Option<String>,
    has_auth: bool,
) -> Result<ResolvedBackendConfig, BackendConfigError> {
    // Resolve api_url: override takes precedence, then constant for centralized envs
    let api_url = api_url_override.or_else(|| env.default_api_url().map(String::from));

    // Auth requires gateway_host
    if has_auth && gateway_host.is_none() {
        return Err(BackendConfigError::AuthRequiresGateway);
    }

    // D14n envs require gateway_host
    if env.is_d14n() && gateway_host.is_none() {
        return Err(BackendConfigError::MissingGatewayHost(env));
    }

    // Derive is_secure from resolved URLs
    let is_secure = match (&api_url, &gateway_host) {
        (Some(url), Some(gw)) => is_url_secure(url) && is_url_secure(gw),
        (Some(url), None) => is_url_secure(url),
        (None, Some(gw)) => is_url_secure(gw),
        (None, None) => false,
    };

    Ok(ResolvedBackendConfig {
        api_url,
        gateway_host,
        is_secure,
        readonly,
        app_version: app_version.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_centralized_env_resolves_api_url() {
        let config = validate_and_resolve(XmtpEnv::Dev, None, None, false, None, false).unwrap();
        assert!(config.api_url.is_some());
        assert!(config.gateway_host.is_none());
        assert!(config.is_secure); // Dev URL is https
    }

    #[test]
    fn test_centralized_env_with_override() {
        let config = validate_and_resolve(
            XmtpEnv::Dev,
            Some("http://custom:5556".to_string()),
            None,
            false,
            None,
            false,
        )
        .unwrap();
        assert_eq!(config.api_url.unwrap(), "http://custom:5556");
        assert!(!config.is_secure); // http, not https
    }

    #[test]
    fn test_d14n_env_requires_gateway_host() {
        let result = validate_and_resolve(XmtpEnv::TestnetStaging, None, None, false, None, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_d14n_env_with_gateway_host() {
        let config = validate_and_resolve(
            XmtpEnv::Testnet,
            None,
            Some("https://gateway.testnet.xmtp.network:443".to_string()),
            false,
            None,
            false,
        )
        .unwrap();
        assert!(config.api_url.is_none());
        assert!(config.gateway_host.is_some());
        assert!(config.is_secure);
    }

    #[test]
    fn test_auth_requires_gateway_host() {
        let result = validate_and_resolve(XmtpEnv::Dev, None, None, false, None, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_auth_with_gateway_host_succeeds() {
        let config = validate_and_resolve(
            XmtpEnv::Dev,
            None,
            Some("https://gateway.dev.xmtp.network:443".to_string()),
            false,
            None,
            true,
        )
        .unwrap();
        assert!(config.api_url.is_some());
        assert!(config.gateway_host.is_some());
    }

    #[test]
    fn test_is_secure_derived_from_urls() {
        let config = validate_and_resolve(
            XmtpEnv::Local,
            Some("http://localhost:5556".to_string()),
            None,
            false,
            None,
            false,
        )
        .unwrap();
        assert!(!config.is_secure); // http URLs are not secure
    }

    #[test]
    fn test_local_env_no_implicit_gateway() {
        let config = validate_and_resolve(XmtpEnv::Local, None, None, false, None, false).unwrap();
        assert!(config.gateway_host.is_none());
    }

    #[test]
    fn test_local_env_explicit_gateway() {
        let config = validate_and_resolve(
            XmtpEnv::Local,
            None,
            Some("http://localhost:5052".to_string()),
            false,
            None,
            false,
        )
        .unwrap();
        assert_eq!(
            config.gateway_host.as_deref(),
            Some("http://localhost:5052")
        );
    }

    #[test]
    fn test_readonly_passthrough() {
        let config = validate_and_resolve(XmtpEnv::Dev, None, None, true, None, false).unwrap();
        assert!(config.readonly);
    }

    #[test]
    fn test_app_version_default() {
        let config = validate_and_resolve(XmtpEnv::Dev, None, None, false, None, false).unwrap();
        assert_eq!(config.app_version, "");
    }

    #[test]
    fn test_local_env_auth_requires_explicit_gateway() {
        // Local env no longer auto-sets gateway_host, so auth without explicit gateway should fail
        let result = validate_and_resolve(XmtpEnv::Local, None, None, false, None, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_url_secure_case_insensitive() {
        let config = validate_and_resolve(
            XmtpEnv::Dev,
            Some("HTTPS://grpc.dev.xmtp.network:443".to_string()),
            None,
            false,
            None,
            false,
        )
        .unwrap();
        assert!(config.is_secure);
    }

    #[test]
    fn test_is_url_secure_grpcs() {
        let config = validate_and_resolve(
            XmtpEnv::Testnet,
            None,
            Some("grpcs://gateway.testnet.xmtp.network:443".to_string()),
            false,
            None,
            false,
        )
        .unwrap();
        assert!(config.is_secure);
    }

    #[test]
    fn test_is_url_secure_grpcs_case_insensitive() {
        let config = validate_and_resolve(
            XmtpEnv::Testnet,
            None,
            Some("GRPCS://gateway.testnet.xmtp.network:443".to_string()),
            false,
            None,
            false,
        )
        .unwrap();
        assert!(config.is_secure);
    }

    #[test]
    fn test_app_version_passthrough() {
        let config = validate_and_resolve(
            XmtpEnv::Dev,
            None,
            None,
            false,
            Some("MyApp/1.0".to_string()),
            false,
        )
        .unwrap();
        assert_eq!(config.app_version, "MyApp/1.0");
    }
}
