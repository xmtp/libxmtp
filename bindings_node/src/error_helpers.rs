use napi::{Env, Result, Status};
use napi::bindgen_prelude::JsObjectValue;
use napi::JsValue;
use napi_derive::napi;
use serde_json::json;

/// Build and throw a JS Error with `code` and `details` properties for GroupError::NotFound.
#[cfg_attr(feature = "test-utils", napi)]
pub fn throw_group_not_found_error(env: Env) -> Result<()> {
  throw_structured_error(
    env,
    "GroupError::NotFound",
    json!({ "entity": "test-entity" }),
  )
}

/// Build and throw a JS Error with `code` and `details` properties for IdentityError::TooManyInstallations.
#[cfg_attr(feature = "test-utils", napi)]
pub fn throw_identity_too_many_installations_error(env: Env) -> Result<()> {
  throw_structured_error(
    env,
    "IdentityError::TooManyInstallations",
    json!({ "inboxId": "test-inbox", "count": 2, "max": 1 }),
  )
}

fn throw_structured_error(env: Env, code: &str, details: serde_json::Value) -> Result<()> {
  let mut js_error = env.create_error(napi::Error::new(
    Status::GenericFailure,
    code.to_string(),
  ))?;

  // Attach code and details properties for JS assertions.
  js_error.set_named_property("code", env.create_string(code)?)?;
  let details_js = env.to_js_value(&details)?;
  js_error.set_named_property("details", details_js)?;

  unsafe {
    napi::sys::napi_throw(env.raw(), js_error.raw());
  }
  Ok(())
}
