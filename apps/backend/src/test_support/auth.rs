//! Signing fixtures and a scripted HTTP key server for backend auth tests.
use crate::config::auth::{AuthConfig, AuthKeyConfig};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use parking_lot::Mutex;
use rsa::{
    pkcs8::{DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding},
    traits::PublicKeyParts,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, LazyLock},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};
use xmtp_common::time::{Duration, Instant};

#[derive(Clone)]
pub struct TestKey {
    pub kid: String,
    pub alg: Algorithm,
    pub public_key: String,
    pub encoding: EncodingKey,
    pub jwk: Value,
}
impl TestKey {
    pub fn es256() -> Self {
        Self::ec_or_ed(Algorithm::ES256)
    }
    pub fn es384() -> Self {
        Self::ec_or_ed(Algorithm::ES384)
    }
    pub fn eddsa() -> Self {
        Self::ec_or_ed(Algorithm::EdDSA)
    }

    /// Generate fresh EC or Ed25519 material for each test.
    fn ec_or_ed(alg: Algorithm) -> Self {
        use p256::elliptic_curve::sec1::ToEncodedPoint;
        let algorithm = match alg {
            Algorithm::ES256 => &rcgen::PKCS_ECDSA_P256_SHA256,
            Algorithm::ES384 => &rcgen::PKCS_ECDSA_P384_SHA384,
            _ => &rcgen::PKCS_ED25519,
        };
        let key = rcgen::KeyPair::generate_for(algorithm).expect("test signing key");
        let public_key = key.public_key_pem();
        let private = key.serialize_pem();
        let kid = uuid::Uuid::new_v4().to_string();
        let (encoding, mut jwk) = match alg {
            Algorithm::ES256 | Algorithm::ES384 => {
                let point = if alg == Algorithm::ES256 {
                    p256::PublicKey::from_public_key_pem(&public_key)
                        .expect("P-256 SPKI")
                        .to_encoded_point(false)
                        .as_bytes()
                        .to_vec()
                } else {
                    p384::PublicKey::from_public_key_pem(&public_key)
                        .expect("P-384 SPKI")
                        .to_encoded_point(false)
                        .as_bytes()
                        .to_vec()
                };
                let width = (point.len() - 1) / 2;
                (
                    EncodingKey::from_ec_pem(private.as_bytes()).expect("EC private key"),
                    json!({"kty": "EC", "crv": if alg == Algorithm::ES256 { "P-256" } else { "P-384" }, "x": URL_SAFE_NO_PAD.encode(&point[1..1+width]), "y": URL_SAFE_NO_PAD.encode(&point[1+width..])}),
                )
            }
            _ => {
                let public = ed25519_dalek::VerifyingKey::from_public_key_pem(&public_key)
                    .expect("Ed25519 SPKI");
                (
                    EncodingKey::from_ed_pem(private.as_bytes()).expect("Ed25519 private key"),
                    json!({"kty": "OKP", "crv": "Ed25519", "x": URL_SAFE_NO_PAD.encode(public.as_bytes())}),
                )
            }
        };
        jwk["kid"] = json!(kid);
        jwk["alg"] = json!(alg);
        jwk["use"] = json!("sig");
        Self {
            kid,
            alg,
            public_key,
            encoding,
            jwk,
        }
    }

    /// Reuse one RSA pair per process because RSA key generation is expensive.
    pub fn rsa() -> Self {
        static KEY: LazyLock<TestKey> = LazyLock::new(|| {
            let private = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)
                .expect("RSA key generation");
            let public = private.to_public_key();
            let pem = private
                .to_pkcs8_pem(LineEnding::LF)
                .expect("RSA private PEM");
            let kid = uuid::Uuid::new_v4().to_string();
            TestKey {
                kid: kid.clone(),
                alg: Algorithm::RS256,
                public_key: public.to_public_key_pem(LineEnding::LF).expect("RSA SPKI"),
                encoding: EncodingKey::from_rsa_pem(pem.as_bytes()).expect("RSA encoding key"),
                jwk: json!({"kty": "RSA", "alg": "RS256", "use": "sig", "kid": kid, "n": URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()), "e": URL_SAFE_NO_PAD.encode(public.e().to_bytes_be())}),
            }
        });
        KEY.clone()
    }
    pub fn config(&self) -> AuthKeyConfig {
        AuthKeyConfig {
            kid: self.kid.clone(),
            alg: format!("{:?}", self.alg),
            public_key: self.public_key.clone(),
        }
    }
    pub fn auth_config(&self) -> AuthConfig {
        AuthConfig {
            keys: Some(vec![self.config()]),
            ..AuthConfig::default()
        }
    }
}

pub fn mint(claims: &impl Serialize, key: &TestKey) -> String {
    let mut header = Header::new(key.alg);
    header.kid = Some(key.kid.clone());
    mint_with_header(claims, key, header)
}
pub fn mint_with_header(claims: &impl Serialize, key: &TestKey, header: Header) -> String {
    jsonwebtoken::encode(&header, claims, &key.encoding).expect("test token")
}
pub fn valid_claims() -> Value {
    json!({"exp": xmtp_common::time::now_secs() + 3600})
}

#[derive(Clone)]
pub struct JwksResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub location: Option<String>,
    pub delay: Duration,
    pub chunked: bool,
}
impl JwksResponse {
    pub fn keys(keys: &[TestKey]) -> Self {
        Self::json(json!({"keys": keys.iter().map(|key| key.jwk.clone()).collect::<Vec<_>>()}))
    }
    pub fn json(value: Value) -> Self {
        Self {
            status: 200,
            body: serde_json::to_vec(&value).expect("JWKS JSON"),
            location: None,
            delay: Duration::ZERO,
            chunked: false,
        }
    }
    pub fn error() -> Self {
        Self {
            status: 503,
            body: b"private-response-sentinel".to_vec(),
            ..Self::json(json!({}))
        }
    }
    pub fn redirect(location: String) -> Self {
        Self {
            status: 302,
            location: Some(location),
            ..Self::json(json!({}))
        }
    }
    pub fn oversized() -> Self {
        Self {
            body: vec![b' '; super::super::auth::jwks::MAX_JWKS_BYTES + 1],
            chunked: true,
            ..Self::json(json!({}))
        }
    }
}

pub struct JwksServer {
    pub url: String,
    requests: Arc<Mutex<Vec<Instant>>>,
    responses: Arc<Mutex<VecDeque<JwksResponse>>>,
    task: JoinHandle<()>,
}
impl JwksServer {
    /// Serve scripted replies. Repeat the final reply until the script is replaced.
    /// Drop cancels the listener and all open response tasks.
    pub async fn start(responses: Vec<JwksResponse>) -> Self {
        assert!(!responses.is_empty());
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("JWKS listener");
        let url = format!(
            "http://{}/keys",
            listener.local_addr().expect("JWKS address")
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let script = Arc::new(Mutex::new(VecDeque::from(responses)));
        let observed = requests.clone();
        let replies = script.clone();
        let task = tokio::spawn(async move {
            let mut tasks = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((mut socket, _)) = accepted else { break; };
                        let observed = observed.clone(); let replies = replies.clone();
                        tasks.spawn(async move {
                            let mut request = Vec::new();
                            let mut byte = [0];
                            while request.len() < 8192 && !request.ends_with(b"\r\n\r\n") {
                                if socket.read_exact(&mut byte).await.is_err() { return; }
                                request.push(byte[0]);
                            }
                            observed.lock().push(Instant::now());
                            let reply = { let mut replies = replies.lock(); if replies.len() > 1 { replies.pop_front().expect("reply") } else { replies.front().expect("reply").clone() } };
                            xmtp_common::time::sleep(reply.delay).await;
                            let mut header = format!("HTTP/1.1 {} Reply\r\nConnection: close\r\nContent-Type: application/json\r\n", reply.status);
                            if let Some(location) = reply.location { header.push_str(&format!("Location: {location}\r\n")); }
                            if reply.chunked { header.push_str("Transfer-Encoding: chunked\r\n\r\n"); } else { header.push_str(&format!("Content-Length: {}\r\n\r\n", reply.body.len())); }
                            if socket.write_all(header.as_bytes()).await.is_err() { return; }
                            if reply.chunked {
                                for chunk in reply.body.chunks(4096) {
                                    if socket.write_all(format!("{:x}\r\n", chunk.len()).as_bytes()).await.is_err() { return; }
                                    if socket.write_all(chunk).await.is_err() { return; }
                                    if socket.write_all(b"\r\n").await.is_err() { return; }
                                }
                                let _ = socket.write_all(b"0\r\n\r\n").await;
                            } else { let _ = socket.write_all(&reply.body).await; }
                        });
                    },
                    _ = tasks.join_next(), if !tasks.is_empty() => {}
                }
            }
        });
        Self {
            url,
            requests,
            responses: script,
            task,
        }
    }
    pub fn requests(&self) -> Vec<Instant> {
        self.requests.lock().clone()
    }
    pub fn set_responses(&self, responses: Vec<JwksResponse>) {
        assert!(!responses.is_empty());
        *self.responses.lock() = responses.into();
    }
    pub fn config(&self) -> AuthConfig {
        AuthConfig {
            jwks_url: Some(self.url.clone()),
            ..AuthConfig::default()
        }
    }
}
impl Drop for JwksServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
