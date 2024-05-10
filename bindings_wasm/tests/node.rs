extern crate bindings_wasm;
extern crate wasm_bindgen_test;

use wasm_bindgen_futures::JsFuture;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}


#[wasm_bindgen_test]
async fn test_async_works() {
    let promise = js_sys::Promise::resolve(&JsValue::from(42));
    let result = JsFuture::from(promise).await.unwrap();
    assert_eq!(result, 42);
}
