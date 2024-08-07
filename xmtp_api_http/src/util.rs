use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use xmtp_proto::api_client::{Error, ErrorKind};

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum GrpcResponse<T> {
  Ok(T),
  Err(ErrorResponse),
  Empty {},
}

#[derive(Deserialize, Serialize)]
struct ErrorResponse {
  code: usize,
  message: String,
  details: Vec<String>,
}

/// handle JSON response from gRPC, returning either
/// the expected deserialized response object or a gRPC [`Error`]
pub fn handle_error<S: AsRef<str>, T>(text: S) -> Result<T, Error>
where
  T: DeserializeOwned + Default,
{
  println!("TEXT: {:?}", text.as_ref());
  match serde_json::from_str(text.as_ref()) {
    Ok(GrpcResponse::Ok(response)) => Ok(response),
    Ok(GrpcResponse::Err(e)) => Err(Error::new(ErrorKind::IdentityError).with(e.message)),
    Err(e) => Err(Error::new(ErrorKind::QueryError).with(e.to_string())),
    Ok(GrpcResponse::Empty {}) => Ok(Default::default()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_error_handler_on_unit_value() {
    handle_error::<_, ()>("{}").unwrap();
  }
}
