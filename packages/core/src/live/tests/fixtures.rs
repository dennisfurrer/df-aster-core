//! Frames recorded from Aster's socket on 2026-11-11 (`fixtures/aster/live_*`), and the
//! account stream's events as the docs give them (`live_user_docs.jsonl`).

/// One frame of each stream, and the answers and refusal the socket gave.
pub(crate) const STREAM: &str = include_str!("../../../fixtures/aster/live_stream.jsonl");
/// ETHUSDT's diff stream at 100 ms, 31 updates in a row.
pub(crate) const DIFFS: &str = include_str!("../../../fixtures/aster/live_depth_diffs.jsonl");
/// `GET /fapi/v3/depth?symbol=ETHUSDT&limit=50`, taken a second into the diffs.
pub(crate) const SNAPSHOT: &str = include_str!("../../../fixtures/aster/live_depth_snapshot.json");
/// The docs' account events, and two every-symbol arrays.
pub(crate) const USER_DOCS: &str = include_str!("../../../fixtures/aster/live_user_docs.jsonl");

/// The recorded frame on `stream`.
pub(crate) fn frame(stream: &str) -> &'static str {
    let tag = format!("{{\"stream\":\"{stream}\"");
    STREAM.lines().find(|line| line.starts_with(&tag)).unwrap_or_else(|| panic!("no frame on {stream}"))
}

/// The docs' event whose `e` is `event`, the `nth` of them.
pub(crate) fn docs(event: &str, nth: usize) -> &'static str {
    let tag = format!("\"e\":\"{event}\"");
    USER_DOCS.lines().filter(|line| line.contains(&tag)).nth(nth).unwrap_or_else(|| panic!("no {event}"))
}
