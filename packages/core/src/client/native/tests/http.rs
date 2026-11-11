//! Against a server on 127.0.0.1 that answers one request with canned bytes and hands back what it
//! was sent. Nothing leaves the machine.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};

use crate::client::{Http, Method, Request, TransportError};

use super::*;

/// Serves one request with `answer`; the request comes back on the channel.
fn serve_once(answer: Vec<u8>) -> (String, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (sent, received) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..read]);
            if let Some(end) = find(&request, b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&request[..end]).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .map_or(0, |length| length.trim().parse::<usize>().unwrap());
                if request.len() >= end + 4 + length {
                    break;
                }
            }
            if read == 0 {
                break;
            }
        }
        stream.write_all(&answer).unwrap();
        sent.send(request).unwrap();
    });
    (url, received)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn answer(status: &str, headers: &[&str], body: &[u8]) -> Vec<u8> {
    let mut head = format!("HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n", body.len());
    for header in headers {
        head.push_str(header);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    [head.into_bytes(), body.to_vec()].concat()
}

/// What the server was sent by `request`, built against its URL.
async fn sent(request: impl FnOnce(&str) -> Request) -> String {
    let (url, received) = serve_once(answer("200 OK", &[], b"[]"));
    let response = Native::new().send(request(&url)).await.unwrap();
    assert_eq!(response.text(), "[]");
    String::from_utf8(received.recv().unwrap()).unwrap()
}

#[tokio::test]
async fn each_request_goes_out_with_its_method_headers_and_body() {
    let body = "symbol=BTCUSDT&batchOrders=%5B%7B%22A%22%3A1%7D%5D".to_owned();
    let post =
        sent(|url| Request::form(Method::Post, format!("{url}/fapi/v3/batchOrders"), body).with_header("X-Key", "k1"))
            .await;
    assert!(post.starts_with("POST /fapi/v3/batchOrders HTTP/1.1\r\n"), "{post}");
    let lower = post.to_lowercase();
    assert!(
        lower.contains("content-type: application/x-www-form-urlencoded\r\n") && lower.contains("x-key: k1\r\n"),
        "{post}"
    );
    assert!(post.ends_with("symbol=BTCUSDT&batchOrders=%5B%7B%22A%22%3A1%7D%5D"), "{post}");

    let put = sent(|url| Request::form(Method::Put, format!("{url}/fapi/v3/listenKey"), "a=1".to_owned())).await;
    assert!(put.starts_with("PUT /fapi/v3/listenKey HTTP/1.1\r\n") && put.ends_with("a=1"), "{put}");
    let delete = sent(|url| Request::new(Method::Delete, format!("{url}/fapi/v3/order?symbol=X"))).await;
    assert!(delete.starts_with("DELETE /fapi/v3/order?symbol=X HTTP/1.1\r\n"), "{delete}");

    let json = sent(|url| Request::post_json(url, r#"{"id":1}"#.to_owned())).await;
    assert!(json.to_lowercase().contains("content-type: application/json\r\n") && json.ends_with(r#"{"id":1}"#));

    let get =
        sent(|url| Request::get(format!("{url}/fapi/v3/trades?symbol=BTCUSDT")).with_header("Authorization", "t"))
            .await;
    assert!(get.starts_with("GET /fapi/v3/trades?symbol=BTCUSDT HTTP/1.1\r\n") && get.ends_with("\r\n\r\n"), "{get}");
    let lower = get.to_lowercase();
    assert!(lower.contains("authorization: t\r\n") && !lower.contains("content-type:"), "{get}");
}

#[tokio::test]
async fn any_answer_comes_back_whole_its_status_headers_and_bytes() {
    let (url, _received) = serve_once(answer("429 Too Many Requests", &["Retry-After: 7"], b"null"));
    let response = Native::new().send(Request::get(url)).await.unwrap();
    assert_eq!((response.status, response.header("retry-after"), response.text()), (429, Some("7"), "null".to_owned()));
    let bytes: Vec<u8> = (0..=255).collect();
    let (url, _received) = serve_once(answer("200 OK", &["content-type: application/octet-stream"], &bytes));
    let response = Native::new().send(Request::post_json(url, "{}".to_owned())).await.unwrap();
    assert_eq!(response.body, bytes);
}

#[tokio::test]
async fn no_answer_is_unreachable() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let error = Native::with_timeout(TIMEOUT).send(Request::post_json(url, "{}".to_owned())).await.unwrap_err();
    assert!(matches!(error, TransportError::Unreachable(_)), "{error:?}");
}

#[test]
fn the_clock_reads_unix_milliseconds() {
    let now = clock();
    assert!(now > 1_790_000_000_000, "after 2026-09: {now}");
    assert!(now < 4_102_444_800_000, "before 2100: {now}");
}

/// An `offline` build refuses what changes the account before it connects, and still reads.
#[cfg(feature = "offline")]
#[tokio::test]
async fn an_offline_build_sends_nothing_signed() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let body = "symbol=BTCUSDT&nonce=1&signature=0x00".to_owned();
    let error =
        Native::new().send(Request::form(Method::Post, format!("{url}/fapi/v3/order"), body)).await.unwrap_err();
    assert!(matches!(error, TransportError::Offline(_)), "{error:?}");
    assert!(listener.accept().is_err(), "nothing connected");

    let read = sent(|url| Request::get(format!("{url}/fapi/v3/balance?nonce=1&signature=0x00"))).await;
    assert!(read.starts_with("GET /fapi/v3/balance?nonce=1&signature=0x00 HTTP/1.1\r\n"), "{read}");
}

/// The warming task ends by itself once the last holder of the client lets go.
#[tokio::test]
async fn keeping_warm_ends_with_the_last_holder() {
    let client = std::sync::Arc::new(crate::Client::reader(Native::new(), clock));
    let warming = keep_warm(&client, KEEP_WARM);
    drop(client);
    tokio::time::timeout(Duration::from_secs(5), warming).await.unwrap().unwrap();
}
