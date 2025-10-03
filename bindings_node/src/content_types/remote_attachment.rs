use napi_derive::napi;

#[derive(Clone)]
#[napi(object)]
pub struct RemoteAttachment {
  pub url: String,
  pub content_digest: String,
  pub secret: Vec<u8>,
  pub salt: Vec<u8>,
  pub nonce: Vec<u8>,
  pub scheme: String,
  pub content_length: i64,
  pub filename: Option<String>,
}

impl From<xmtp_content_types::remote_attachment::RemoteAttachment> for RemoteAttachment {
  fn from(remote: xmtp_content_types::remote_attachment::RemoteAttachment) -> Self {
    Self {
      url: remote.url,
      content_digest: remote.content_digest,
      secret: remote.secret,
      salt: remote.salt,
      nonce: remote.nonce,
      scheme: remote.scheme,
      content_length: remote.content_length as i64,
      filename: remote.filename,
    }
  }
}
