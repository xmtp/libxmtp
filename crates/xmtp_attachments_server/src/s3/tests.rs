use super::*;
use aws_credential_types::provider::{Result as CredentialResult, future};
use std::{
    collections::VecDeque,
    ffi::OsStr,
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
    // SAFETY: Nextest runs each test in its own process. Set variables before creating providers.
    unsafe { std::env::set_var(key, value) };
}

fn remove_test_env(key: &str) {
    // SAFETY: Nextest runs each test in its own process. Remove variables before creating providers.
    unsafe { std::env::remove_var(key) };
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
#[xmtp_common::test(unwrap_try = true)]
async fn every_credential_kind_builds() {
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
    for credentials in kinds {
        let mut config = config();
        config.credentials = credentials;
        assert!(
            S3Target::new(&config).await.is_ok(),
            "could not build {:?}",
            config.credentials
        );
    }
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true)]
async fn environment_credentials_resolve() {
    set_test_env("AWS_ACCESS_KEY_ID", "ENV_KEY");
    set_test_env("AWS_SECRET_ACCESS_KEY", "ENV_SECRET");
    remove_test_env("AWS_SESSION_TOKEN");
    assert_resolves_to(CredentialsConfig::Environment, "ENV_KEY").await?;
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true)]
async fn default_chain_credentials_resolve_from_environment() {
    set_test_env("AWS_EC2_METADATA_DISABLED", "true");
    set_test_env("AWS_ACCESS_KEY_ID", "CHAIN_KEY");
    set_test_env("AWS_SECRET_ACCESS_KEY", "CHAIN_SECRET");
    remove_test_env("AWS_SESSION_TOKEN");
    assert_resolves_to(CredentialsConfig::DefaultChain, "CHAIN_KEY").await?;
}

// verifies: ATCH-073
#[xmtp_common::test(unwrap_try = true)]
async fn profile_credentials_resolve() {
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
    assert_resolves_to(
        CredentialsConfig::Profile {
            name: "fixture".into(),
        },
        "PROFILE_KEY",
    )
    .await?;
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
#[xmtp_common::test(unwrap_try = true)]
async fn container_credentials_resolve() {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/credentials", listener.local_addr()?);
    remove_test_env("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI");
    remove_test_env("AWS_CONTAINER_AUTHORIZATION_TOKEN");
    remove_test_env("AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE");
    set_test_env("AWS_CONTAINER_CREDENTIALS_FULL_URI", url);

    let serve = async {
        let (mut stream, _) = listener.accept().await?;
        let mut request = [0; 2048];
        let size = stream.read(&mut request).await?;
        assert!(String::from_utf8_lossy(&request[..size]).starts_with("GET /credentials "));
        let body = r#"{"AccessKeyId":"CONTAINER_KEY","SecretAccessKey":"CONTAINER_SECRET","Token":"CONTAINER_TOKEN","Expiration":"2099-01-01T00:00:00Z"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await?;
        Ok::<(), std::io::Error>(())
    };
    let (signed, served) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            assert_resolves_to(CredentialsConfig::Container, "CONTAINER_KEY"),
            serve
        )
    })
    .await?;
    signed?;
    served?;
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
