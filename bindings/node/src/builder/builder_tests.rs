/// Test builder structs for verifying the `#[napi_builder]` macro from JS.
///
/// These structs are only compiled when the `test-utils` feature is enabled
/// (via `yarn build:test`). They exercise all builder field modes:
/// required, optional, default, and skip.
///
/// The TypeScript tests in `test/Builder.test.ts` import these structs and
/// verify constructor parameters, setter chaining, and default values.
#[allow(dead_code)]
#[xmtp_macro::napi_builder]
pub struct NapiTestBuilder {
  #[builder(required)]
  pub name: String,

  pub flag: Option<bool>,

  pub count: Option<u32>,

  #[builder(default = "42")]
  pub port: u32,

  #[builder(default = "true")]
  pub enabled: bool,

  #[builder(skip)]
  internal: Vec<u8>,
}
