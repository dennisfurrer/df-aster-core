//! Run as wasm in Node (`just web`), through Node's own `fetch`. `data:` URLs answer without any
//! network; a server on 127.0.0.1 started from the test (`echo.js`) hands back what it was sent; a
//! port nothing listens on is no answer.

use crate::client::{Http, Method, Request, TransportError};
use js_sys::Promise;
use serde_json::Value;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;

use super::*;

#[wasm_bindgen(module = "/fixtures/web/echo.js")]
extern "C" {
    fn echo() -> Promise;
}

/// What the echo server was sent by `request` to its URL, as it read it.
async fn echoed(request: impl FnOnce(&str) -> Request) -> Value {
    let url = JsFuture::from(echo()).await.unwrap().as_string().unwrap();
    let response = Web.send(request(&url)).await.unwrap();
    serde_json::from_slice(&response.body).unwrap()
}

#[wasm_bindgen_test]
async fn an_answer_comes_back_whole_its_status_headers_and_bytes() {
    for request in [
        Request::post_json(r#"data:application/json,{"ok":true}"#, "{}".to_owned()),
        Request::get(r#"data:application/json,{"ok":true}"#),
    ] {
        let response = Web.send(request).await.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.header("Content-Type"), Some("application/json"));
        assert_eq!(response.text(), r#"{"ok":true}"#);
    }
    let request = Request::post_json("data:application/octet-stream;base64,AAH/gA==", "{}".to_owned());
    assert_eq!(Web.send(request).await.unwrap().body, vec![0x00, 0x01, 0xff, 0x80]);
}

#[wasm_bindgen_test]
async fn each_request_goes_out_with_its_method_headers_and_body() {
    let body = "symbol=BTCUSDT&batchOrders=%5B%7B%22A%22%3A1%7D%5D".to_owned();
    let post = echoed(|url| {
        Request::form(Method::Post, format!("{url}/fapi/v3/batchOrders"), body).with_header("X-Key", "k1")
    })
    .await;
    assert_eq!((&post["method"], &post["url"]), (&Value::from("POST"), &Value::from("/fapi/v3/batchOrders")));
    assert_eq!(
        (&post["headers"]["content-type"], &post["headers"]["x-key"]),
        (&Value::from("application/x-www-form-urlencoded"), &Value::from("k1"))
    );
    assert_eq!(post["body"], "symbol=BTCUSDT&batchOrders=%5B%7B%22A%22%3A1%7D%5D");
    let delete = echoed(|url| Request::new(Method::Delete, format!("{url}/fapi/v3/order?symbol=X"))).await;
    assert_eq!((&delete["method"], &delete["url"]), (&Value::from("DELETE"), &Value::from("/fapi/v3/order?symbol=X")));
    let get =
        echoed(|url| Request::get(format!("{url}/fapi/v3/trades?symbol=BTCUSDT")).with_header("Authorization", "t"))
            .await;
    assert_eq!((&get["method"], &get["url"]), (&Value::from("GET"), &Value::from("/fapi/v3/trades?symbol=BTCUSDT")));
    assert_eq!(get["headers"]["authorization"], "t");
    assert_eq!((get["headers"].get("content-type"), &get["body"]), (None, &Value::from("")));
}

#[wasm_bindgen_test]
async fn no_answer_is_unreachable() {
    let error = Web.send(Request::get("http://127.0.0.1:9/fapi/v3/ping")).await.unwrap_err();
    assert!(matches!(error, TransportError::Unreachable(_)), "{error:?}");
}

#[wasm_bindgen_test]
fn the_clock_reads_unix_milliseconds() {
    let now = clock();
    assert!(now > 1_790_000_000_000, "after 2026-09: {now}");
    assert!(now < 4_102_444_800_000, "before 2100: {now}");
}
