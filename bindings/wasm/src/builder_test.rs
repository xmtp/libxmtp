/// Test builder structs for verifying the `#[wasm_builder]` macro from JS.
///
/// These structs are only compiled when the `test-utils` feature is enabled
/// (via `yarn build:test`). They exercise all builder field modes:
/// required, optional, default, and skip.
///
/// The TypeScript tests in `test/Builder.test.ts` import these structs and
/// verify constructor parameters, setter chaining, and default values.

// -- Combined test struct exercising all field modes -----------------------
#[xmtp_macro::wasm_builder]
pub struct WasmTestBuilder {
  #[builder(required)]
  pub name: String,

  pub flag: Option<bool>,

  pub count: Option<u32>,

  #[builder(default = "42")]
  pub port: u32,

  #[builder(default = "true")]
  pub enabled: bool,
}
