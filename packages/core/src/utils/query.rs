//! The query encoding Aster's signatures cover: JavaScript's `encodeURIComponent`, which Aster's
//! web app writes every signed call with. The signature covers the encoded bytes, so the request
//! carries exactly the string that was signed.

/// What `encodeURIComponent` leaves as it is.
const fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')')
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// `encodeURIComponent`: UTF-8 bytes outside the unreserved set as `%XX`, upper case.
#[must_use]
pub fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if unreserved(byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    out
}

/// `k=v&k=v`, keys and values encoded, in the given order.
#[must_use]
pub fn query(params: &[(&str, String)]) -> String {
    let mut out = String::new();
    for (at, (key, value)) in params.iter().enumerate() {
        if at > 0 {
            out.push('&');
        }
        out.push_str(&encode_component(key));
        out.push('=');
        out.push_str(&encode_component(value));
    }
    out
}

#[cfg(test)]
#[path = "tests/query.rs"]
mod tests;
