use super::*;
use aws_credential_types::provider::{Result as CredentialResult, future};
use std::{
    collections::VecDeque,
    ffi::OsStr,
    future::Future,
    io,
    sync::{
        Mutex as StdMutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::UNIX_EPOCH,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const REFERENCE_TIME: u64 = 1_369_353_600; // 2013-05-24T00:00:00Z

fn config() -> S3Config {
    S3Config {
        endpoint: "https://s3.example.com".into(),
        region: "us-east-1".into(),
        bucket: "attachments".into(),
        key_prefix: "prefix/".into(),
        credentials: CredentialsConfig::Static {
            access_key_id: "AKID".into(),
            secret_access_key: "SECRET".into(),
            session_token: Some("SESSION_TOKEN_SECRET".into()),
        },
        presign_ttl_seconds: Some(900),
    }
}

fn fixed_clock() -> Arc<dyn Fn() -> SystemTime + Send + Sync> {
    Arc::new(|| UNIX_EPOCH + Duration::from_secs(REFERENCE_TIME))
}

fn credentials(expiry_after: Option<u64>) -> Credentials {
    Credentials::new(
        "AKID",
        "SECRET",
        None,
        expiry_after.map(|seconds| UNIX_EPOCH + Duration::from_secs(REFERENCE_TIME + seconds)),
        "test",
    )
}

#[derive(Debug)]
struct FakeProvider {
    replies: StdMutex<VecDeque<CredentialResult>>,
    calls: AtomicUsize,
    advance_on_call: Option<(usize, Arc<AtomicU64>, u64)>,
}

impl FakeProvider {
    fn new(replies: impl IntoIterator<Item = Credentials>) -> Arc<Self> {
        Self::with_results(replies.into_iter().map(Ok))
    }

    fn with_results(replies: impl IntoIterator<Item = CredentialResult>) -> Arc<Self> {
        Arc::new(Self {
            replies: StdMutex::new(replies.into_iter().collect()),
            calls: AtomicUsize::new(0),
            advance_on_call: None,
        })
    }

    fn advancing(
        replies: impl IntoIterator<Item = Credentials>,
        call: usize,
        clock: Arc<AtomicU64>,
        offset: u64,
    ) -> Arc<Self> {
        Arc::new(Self {
            replies: StdMutex::new(replies.into_iter().map(Ok).collect()),
            calls: AtomicUsize::new(0),
            advance_on_call: Some((call, clock, offset)),
        })
    }
}

impl ProvideCredentials for FakeProvider {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some((selected_call, clock, offset)) = &self.advance_on_call
            && call == *selected_call
        {
            clock.store(*offset, Ordering::SeqCst);
        }
        let result: CredentialResult =
            self.replies.lock().unwrap().pop_front().unwrap_or_else(|| {
                Err(
                    aws_credential_types::provider::error::CredentialsError::provider_error(
                        "no credentials",
                    ),
                )
            });
        future::ProvideCredentials::ready(result)
    }
}

fn target(
    config: &S3Config,
    provider: Arc<FakeProvider>,
    clock: Arc<dyn Fn() -> SystemTime + Send + Sync>,
) -> S3Target {
    S3Target::with_provider(
        config,
        SharedCredentialsProvider::from(provider as Arc<dyn ProvideCredentials>),
        clock,
    )
}

fn set_test_env(key: &str, value: impl AsRef<OsStr>) {
    assert_eq!(
        std::env::var("NEXTEST_EXECUTION_MODE").as_deref(),
        Ok("process-per-test"),
        "run with nextest (`just test crate xmtp_attachments_server`)"
    );
    // SAFETY: The assertion requires a separate process for each test. Each caller
    // sets variables before it creates a Tokio runtime or AWS provider.
    unsafe { std::env::set_var(key, value) };
}

fn remove_test_env(key: &str) {
    assert_eq!(
        std::env::var("NEXTEST_EXECUTION_MODE").as_deref(),
        Ok("process-per-test"),
        "run with nextest (`just test crate xmtp_attachments_server`)"
    );
    // SAFETY: The assertion requires a separate process for each test. Each caller
    // removes variables before it creates a Tokio runtime or AWS provider.
    unsafe { std::env::remove_var(key) };
}

fn run_after_env_setup(future: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future);
}

fn listener_before_runtime() -> io::Result<std::net::TcpListener> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

async fn assert_resolves_to(
    credentials: CredentialsConfig,
    access_key_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = config();
    config.credentials = credentials;
    let target = S3Target::new(&config).await?;
    let signed = target.presign_put(&[1; 32], 1).await?;
    let url = Url::parse(&signed.url)?;
    let credential = url
        .query_pairs()
        .find(|(name, _)| name == "X-Amz-Credential")
        .expect("signed URL has a credential scope")
        .1;
    assert_eq!(credential.split('/').next(), Some(access_key_id));
    Ok(())
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
fn every_credential_kind_builds() {
    set_test_env("AWS_EC2_METADATA_DISABLED", "true");
    let kinds = [
        CredentialsConfig::Static {
            access_key_id: "STATIC_KEY".into(),
            secret_access_key: "STATIC_SECRET".into(),
            session_token: None,
        },
        CredentialsConfig::DefaultChain,
        CredentialsConfig::Environment,
        CredentialsConfig::Profile {
            name: "fixture".into(),
        },
        CredentialsConfig::Sso {
            account_id: "123456789012".into(),
            region: "us-east-1".into(),
            role_name: "attachments".into(),
            start_url: "https://sso.example.com/start".into(),
            session_name: None,
        },
        CredentialsConfig::Process {
            command: "printf '{}'".into(),
        },
        CredentialsConfig::WebIdentity,
        CredentialsConfig::Container,
        CredentialsConfig::Instance,
        CredentialsConfig::AssumeRole {
            role_arn: "arn:aws:iam::123456789012:role/attachments".into(),
            external_id: None,
            session_name: Some("attachment-test".into()),
        },
    ];
    run_after_env_setup(async {
        for credentials in kinds {
            let mut config = config();
            config.credentials = credentials;
            assert!(
                S3Target::new(&config).await.is_ok(),
                "could not build {:?}",
                config.credentials
            );
        }
    });
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
fn environment_credentials_resolve() {
    set_test_env("AWS_ACCESS_KEY_ID", "ENV_KEY");
    set_test_env("AWS_SECRET_ACCESS_KEY", "ENV_SECRET");
    remove_test_env("AWS_SESSION_TOKEN");
    run_after_env_setup(async {
        assert_resolves_to(CredentialsConfig::Environment, "ENV_KEY").await?;
    });
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
fn default_chain_credentials_resolve_from_environment() {
    set_test_env("AWS_EC2_METADATA_DISABLED", "true");
    set_test_env("AWS_ACCESS_KEY_ID", "CHAIN_KEY");
    set_test_env("AWS_SECRET_ACCESS_KEY", "CHAIN_SECRET");
    remove_test_env("AWS_SESSION_TOKEN");
    run_after_env_setup(async {
        assert_resolves_to(CredentialsConfig::DefaultChain, "CHAIN_KEY").await?;
    });
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
fn profile_credentials_resolve() {
    let directory = tempfile::tempdir()?;
    let config_file = directory.path().join("config");
    let credentials_file = directory.path().join("credentials");
    std::fs::write(&config_file, "[profile fixture]\nregion = us-east-1\n")?;
    std::fs::write(
        &credentials_file,
        "[fixture]\naws_access_key_id = PROFILE_KEY\naws_secret_access_key = PROFILE_SECRET\n",
    )?;
    set_test_env("AWS_CONFIG_FILE", &config_file);
    set_test_env("AWS_SHARED_CREDENTIALS_FILE", &credentials_file);
    run_after_env_setup(async {
        assert_resolves_to(
            CredentialsConfig::Profile {
                name: "fixture".into(),
            },
            "PROFILE_KEY",
        )
        .await?;
    });
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true)]
async fn process_credentials_resolve() {
    assert_resolves_to(
        CredentialsConfig::Process {
            command: r#"printf '%s' '{"Version":1,"AccessKeyId":"PROCESS_KEY","SecretAccessKey":"PROCESS_SECRET"}'"#.into(),
        },
        "PROCESS_KEY",
    )
    .await?;
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
fn container_credentials_resolve() {
    let listener = listener_before_runtime()?;
    let url = format!("http://{}/credentials", listener.local_addr()?);
    remove_test_env("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI");
    remove_test_env("AWS_CONTAINER_AUTHORIZATION_TOKEN");
    remove_test_env("AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE");
    set_test_env("AWS_CONTAINER_CREDENTIALS_FULL_URI", url);

    run_after_env_setup(async {
        let listener = TcpListener::from_std(listener)?;
        let serve = serve_credential_request(
            &listener,
            "GET /credentials ",
            "application/json",
            r#"{"AccessKeyId":"CONTAINER_KEY","SecretAccessKey":"CONTAINER_SECRET","Token":"CONTAINER_TOKEN","Expiration":"2099-01-01T00:00:00Z"}"#,
        );
        let (signed, served) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(
                assert_resolves_to(CredentialsConfig::Container, "CONTAINER_KEY"),
                serve
            )
        })
        .await?;
        signed?;
        served?;
    });
}

async fn read_credential_request(stream: &mut tokio::net::TcpStream) -> io::Result<Vec<u8>> {
    const MAX_REQUEST_BYTES: usize = 64 * 1024;
    let mut request = Vec::new();
    let mut chunk = [0; 2048];
    let header_end = loop {
        let size = stream.read(&mut chunk).await?;
        if size == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        request.extend_from_slice(&chunk[..size]);
        if request.len() > MAX_REQUEST_BYTES {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        if let Some(start) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break start + 4;
        }
    };
    let headers = std::str::from_utf8(&request[..header_end])
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    let mut content_length = 0;
    for line in headers.lines() {
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value
                .trim()
                .parse::<usize>()
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        }
    }
    if content_length > MAX_REQUEST_BYTES - header_end {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    while request.len() < header_end + content_length {
        let size = stream.read(&mut chunk).await?;
        if size == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        request.extend_from_slice(&chunk[..size]);
    }
    Ok(request)
}

async fn serve_credential_request(
    listener: &TcpListener,
    request_prefix: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let (mut stream, _) = listener.accept().await?;
    let request = read_credential_request(&mut stream).await?;
    assert!(
        request.starts_with(request_prefix.as_bytes()),
        "credential provider sent an unexpected request"
    );
    let token_ttl = if request_prefix == "PUT /latest/api/token " {
        "x-aws-ec2-metadata-token-ttl-seconds: 21600\r\n"
    } else {
        ""
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{token_ttl}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await
}

#[xmtp_common::test(unwrap_try = true)]
async fn credential_fake_waits_for_complete_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let serve = serve_credential_request(&listener, "POST / ", "text/plain", "ok");
    let send = async {
        let mut stream = tokio::net::TcpStream::connect(address).await?;
        let mut response_byte = [0; 1];
        stream.write_all(b"PO").await?;
        assert!(
            tokio::time::timeout(Duration::from_millis(25), stream.read(&mut response_byte))
                .await
                .is_err()
        );
        stream
            .write_all(b"ST / HTTP/1.1\r\nContent-Length: 4\r\n\r\nab")
            .await?;
        assert!(
            tokio::time::timeout(Duration::from_millis(25), stream.read(&mut response_byte))
                .await
                .is_err()
        );
        stream.write_all(b"cd").await?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await?;
        assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
        Ok::<(), io::Error>(())
    };
    let (served, sent) =
        tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(serve, send) }).await?;
    served?;
    sent?;
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true)]
async fn static_credentials_resolve() {
    assert_resolves_to(
        CredentialsConfig::Static {
            access_key_id: "STATIC_KEY".into(),
            secret_access_key: "STATIC_SECRET".into(),
            session_token: None,
        },
        "STATIC_KEY",
    )
    .await?;
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
fn instance_credentials_resolve() {
    let listener = listener_before_runtime()?;
    set_test_env(
        "AWS_EC2_METADATA_SERVICE_ENDPOINT",
        format!("http://{}", listener.local_addr()?),
    );
    set_test_env("AWS_EC2_METADATA_DISABLED", "false");
    run_after_env_setup(async {
        let listener = TcpListener::from_std(listener)?;
        let serve = async {
            serve_credential_request(
                &listener,
                "PUT /latest/api/token ",
                "text/plain",
                "fixture-token",
            )
            .await?;
            serve_credential_request(
                &listener,
                "GET /latest/meta-data/iam/security-credentials/ ",
                "text/plain",
                "fixture-role",
            )
            .await?;
            serve_credential_request(
                &listener,
                "GET /latest/meta-data/iam/security-credentials/fixture-role ",
                "application/json",
                r#"{"Code":"Success","LastUpdated":"2026-01-01T00:00:00Z","Type":"AWS-HMAC","AccessKeyId":"INSTANCE_KEY","SecretAccessKey":"INSTANCE_SECRET","Token":"INSTANCE_TOKEN","Expiration":"2099-01-01T00:00:00Z"}"#,
            )
            .await
        };
        let (signed, served) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(
                assert_resolves_to(CredentialsConfig::Instance, "INSTANCE_KEY"),
                serve
            )
        })
        .await?;
        signed?;
        served?;
    });
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
fn assume_role_credentials_resolve() {
    let listener = listener_before_runtime()?;
    set_test_env(
        "AWS_ENDPOINT_URL_STS",
        format!("http://{}", listener.local_addr()?),
    );
    set_test_env("AWS_ACCESS_KEY_ID", "SOURCE_KEY");
    set_test_env("AWS_SECRET_ACCESS_KEY", "SOURCE_SECRET");
    remove_test_env("AWS_SESSION_TOKEN");
    set_test_env("AWS_EC2_METADATA_DISABLED", "true");
    run_after_env_setup(async {
        let listener = TcpListener::from_std(listener)?;
        let serve = serve_credential_request(
            &listener,
            "POST / ",
            "text/xml",
            r#"<AssumeRoleResponse><AssumeRoleResult><Credentials><AccessKeyId>ASSUME_KEY</AccessKeyId><SecretAccessKey>ASSUME_SECRET</SecretAccessKey><SessionToken>ASSUME_TOKEN</SessionToken><Expiration>2099-01-01T00:00:00Z</Expiration></Credentials></AssumeRoleResult></AssumeRoleResponse>"#,
        );
        let (signed, served) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(
                assert_resolves_to(
                    CredentialsConfig::AssumeRole {
                        role_arn: "arn:aws:iam::123456789012:role/attachments".into(),
                        external_id: None,
                        session_name: Some("attachment-test".into()),
                    },
                    "ASSUME_KEY",
                ),
                serve
            )
        })
        .await?;
        signed?;
        served?;
    });
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
fn web_identity_credentials_resolve() {
    let directory = tempfile::tempdir()?;
    let token_file = directory.path().join("token");
    std::fs::write(&token_file, "fixture-web-token")?;
    let listener = listener_before_runtime()?;
    set_test_env(
        "AWS_ENDPOINT_URL_STS",
        format!("http://{}", listener.local_addr()?),
    );
    set_test_env("AWS_WEB_IDENTITY_TOKEN_FILE", &token_file);
    set_test_env("AWS_ROLE_ARN", "arn:aws:iam::123456789012:role/attachments");
    set_test_env("AWS_ROLE_SESSION_NAME", "attachment-test");
    run_after_env_setup(async {
        let listener = TcpListener::from_std(listener)?;
        let serve = serve_credential_request(
            &listener,
            "POST / ",
            "text/xml",
            r#"<AssumeRoleWithWebIdentityResponse><AssumeRoleWithWebIdentityResult><Credentials><AccessKeyId>WEB_KEY</AccessKeyId><SecretAccessKey>WEB_SECRET</SecretAccessKey><SessionToken>WEB_TOKEN</SessionToken><Expiration>2099-01-01T00:00:00Z</Expiration></Credentials></AssumeRoleWithWebIdentityResult></AssumeRoleWithWebIdentityResponse>"#,
        );
        let (signed, served) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(
                assert_resolves_to(CredentialsConfig::WebIdentity, "WEB_KEY"),
                serve
            )
        })
        .await?;
        signed?;
        served?;
    });
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
fn sso_credentials_resolve() {
    let directory = tempfile::tempdir()?;
    let cache = directory.path().join(".aws/sso/cache");
    std::fs::create_dir_all(&cache)?;
    // The AWS SDK names this cache entry with SHA-1 of the start URL.
    let start_url = "https://d-92671207e4.awsapps.com/start";
    std::fs::write(
        cache.join("13f9d35043871d073ab260e020f0ffde092cb14b.json"),
        r#"{"accessToken":"fixture-sso-token","expiresAt":"2099-01-01T00:00:00Z"}"#,
    )?;
    let listener = listener_before_runtime()?;
    set_test_env("HOME", directory.path());
    set_test_env(
        "AWS_ENDPOINT_URL_SSO",
        format!("http://{}", listener.local_addr()?),
    );
    run_after_env_setup(async {
        let listener = TcpListener::from_std(listener)?;
        let serve = serve_credential_request(
            &listener,
            "GET /federation/credentials?",
            "application/json",
            r#"{"roleCredentials":{"accessKeyId":"SSO_KEY","secretAccessKey":"SSO_SECRET","sessionToken":"SSO_TOKEN","expiration":4070908800000}}"#,
        );
        let (signed, served) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(
                assert_resolves_to(
                    CredentialsConfig::Sso {
                        account_id: "123456789012".into(),
                        region: "us-east-1".into(),
                        role_name: "attachments".into(),
                        start_url: start_url.into(),
                        session_name: None,
                    },
                    "SSO_KEY",
                ),
                serve
            )
        })
        .await?;
        signed?;
        served?;
    });
}

// verifies: ATCH-023
#[xmtp_common::test(unwrap_try = true)]
async fn signed_headers_are_exact() {
    let digest = [0xab; 32];
    let target = target(
        &config(),
        FakeProvider::new([credentials(None)]),
        fixed_clock(),
    );
    let request = target.presign_put(&digest, 1234).await?;
    assert_eq!(request.method, "PUT");
    assert_eq!(
        request.headers,
        vec![
            ("content-length".into(), "1234".into()),
            ("if-none-match".into(), "*".into()),
            ("x-amz-checksum-sha256".into(), STANDARD.encode(digest)),
        ]
    );
    let url = Url::parse(&request.url)?;
    assert_eq!(
        url.path(),
        format!("/attachments/prefix/{}", hex::encode(digest))
    );
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(
        query["X-Amz-SignedHeaders"],
        "content-length;host;if-none-match;x-amz-checksum-sha256"
    );
    assert_eq!(
        query["X-Amz-Expires"],
        request.expires_in_seconds.to_string()
    );
    assert_eq!(request.expires_in_seconds, 900);
}

// AWS S3 SigV4 query-string example: https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sigv4-query-string-auth.html
#[xmtp_common::test(unwrap_try = true)]
async fn aws_documented_example() {
    let credentials = Credentials::new(
        "AKIAIOSFODNN7EXAMPLE",
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        None,
        None,
        "aws-reference",
    );
    let url = Url::parse("https://examplebucket.s3.amazonaws.com/test.txt")?;
    let signed = sign_request(
        "GET",
        &url,
        &[],
        &credentials,
        "us-east-1",
        UNIX_EPOCH + Duration::from_secs(REFERENCE_TIME),
        86400,
    )?;
    let signed = Url::parse(&signed)?;
    assert_eq!(
        signed
            .query_pairs()
            .find(|(key, _)| key == "X-Amz-Signature")
            .unwrap()
            .1,
        "aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"
    );
}

// verifies: ATCH-028
#[xmtp_common::test(unwrap_try = true)]
async fn ttl_capped_by_credentials() {
    let provider = FakeProvider::new([credentials(Some(450))]);
    let target = target(&config(), provider.clone(), fixed_clock());
    let signed = target.presign_put(&[1; 32], 1).await?;
    assert_eq!(signed.expires_in_seconds, 450);
    let url = Url::parse(&signed.url)?;
    assert_eq!(
        url.query_pairs()
            .find(|(key, _)| key == "X-Amz-Expires")
            .unwrap()
            .1,
        "450"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

// verifies: ATCH-028
#[xmtp_common::test(unwrap_try = true)]
async fn refresh_below_300s() {
    let offset = Arc::new(AtomicU64::new(0));
    let clock: Arc<dyn Fn() -> SystemTime + Send + Sync> = {
        let offset = offset.clone();
        Arc::new(move || {
            UNIX_EPOCH + Duration::from_secs(REFERENCE_TIME + offset.load(Ordering::SeqCst))
        })
    };
    let provider = FakeProvider::new([
        credentials(Some(301)),
        credentials(Some(800)),
        credentials(Some(100)),
    ]);
    let target = target(&config(), provider.clone(), clock);
    assert_eq!(
        target.presign_put(&[1; 32], 1).await?.expires_in_seconds,
        301
    );
    offset.store(2, Ordering::SeqCst);
    assert_eq!(
        target.presign_put(&[1; 32], 1).await?.expires_in_seconds,
        798
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    offset.store(600, Ordering::SeqCst);
    assert!(matches!(
        target.presign_put(&[1; 32], 1).await,
        Err(SignError::CredentialsUnavailable)
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
}

// verifies: ATCH-028
#[xmtp_common::test(unwrap_try = true)]
async fn refresh_uses_post_provider_clock() {
    let offset = Arc::new(AtomicU64::new(0));
    let clock: Arc<dyn Fn() -> SystemTime + Send + Sync> = {
        let offset = offset.clone();
        Arc::new(move || {
            UNIX_EPOCH + Duration::from_secs(REFERENCE_TIME + offset.load(Ordering::SeqCst))
        })
    };
    let provider = FakeProvider::advancing(
        [credentials(Some(301)), credentials(Some(800))],
        2,
        offset.clone(),
        100,
    );
    let target = target(&config(), provider.clone(), clock);
    assert_eq!(
        target.presign_put(&[1; 32], 1).await?.expires_in_seconds,
        301
    );
    offset.store(2, Ordering::SeqCst);
    let signed = target.presign_put(&[1; 32], 1).await?;
    assert_eq!(signed.expires_in_seconds, 700);
    let url = Url::parse(&signed.url)?;
    assert_eq!(
        url.query_pairs()
            .find(|(key, _)| key == "X-Amz-Expires")
            .unwrap()
            .1,
        "700"
    );
    assert_eq!(
        url.query_pairs()
            .find(|(key, _)| key == "X-Amz-Date")
            .unwrap()
            .1,
        "20130524T000140Z"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
}

// verifies: ATCH-028
#[xmtp_common::test(unwrap_try = true)]
async fn presign_uses_one_clock_read_per_credentials_decision() {
    let reads = Arc::new(AtomicUsize::new(0));
    let clock: Arc<dyn Fn() -> SystemTime + Send + Sync> = {
        let reads = reads.clone();
        Arc::new(move || {
            let offset = reads.fetch_add(1, Ordering::SeqCst) as u64;
            UNIX_EPOCH + Duration::from_secs(REFERENCE_TIME + offset)
        })
    };
    let target = target(
        &config(),
        FakeProvider::new([credentials(Some(800))]),
        clock,
    );
    let first = target.presign_put(&[1; 32], 1).await?;
    assert_eq!(first.expires_in_seconds, 799);
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    let second = target.presign_put(&[2; 32], 1).await?;
    assert_eq!(second.expires_in_seconds, 798);
    assert_eq!(reads.load(Ordering::SeqCst), 3);
}

#[xmtp_common::test(unwrap_try = true)]
async fn refresh_error_does_not_use_cached_credentials() {
    let offset = Arc::new(AtomicU64::new(0));
    let clock: Arc<dyn Fn() -> SystemTime + Send + Sync> = {
        let offset = offset.clone();
        Arc::new(move || {
            UNIX_EPOCH + Duration::from_secs(REFERENCE_TIME + offset.load(Ordering::SeqCst))
        })
    };
    let provider = FakeProvider::with_results([
        Ok(credentials(Some(301))),
        Err(
            aws_credential_types::provider::error::CredentialsError::provider_error(
                "provider failed",
            ),
        ),
    ]);
    let target = target(&config(), provider.clone(), clock);
    target.presign_put(&[1; 32], 1).await?;
    offset.store(2, Ordering::SeqCst);
    assert!(matches!(
        target.presign_put(&[1; 32], 1).await,
        Err(SignError::CredentialsUnavailable)
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
}

#[xmtp_common::test(unwrap_try = true)]
async fn debug_redacts_secrets() {
    let config = config();
    let full_config = AttachmentsConfig {
        base_url: "https://public.example.com/objects".into(),
        max_upload_bytes: None,
        retention_seconds: None,
        target: TargetConfig::S3(config.clone()),
    };
    let provider = FakeProvider::new([credentials(None)]);
    let target = target(&config, provider, fixed_clock());
    let signed = target.presign_put(&[1; 32], 1).await?;
    for output in [
        format!("{full_config:?}"),
        format!("{config:?}"),
        format!("{target:?}"),
        format!("{signed:?}"),
    ] {
        for secret in [
            "SECRET",
            "AKID",
            "SESSION_TOKEN_SECRET",
            "s3.example.com",
            "attachments",
            "X-Amz-Signature",
        ] {
            assert!(!output.contains(secret), "Debug exposed {secret}");
        }
    }
    assert_eq!(
        format!("{:?}", SignError::CredentialsUnavailable),
        "CredentialsUnavailable"
    );
}
