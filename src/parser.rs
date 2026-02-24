use alloc::string::String;

use crate::types::HttpHeader;
use crate::util::find_subslice;

pub(crate) fn build_head_line(
    headers: &[HttpHeader<'_>],
    bearer_token: Option<&str>,
) -> Result<String, &'static str> {
    if headers.is_empty() && bearer_token.is_none() {
        return Ok(String::new());
    }

    let mut out = String::new();
    let mut first = true;

    for h in headers {
        if h.name.is_empty() {
            return Err("header name 不能为空");
        }
        if !first {
            out.push_str("[0D][0A]");
        }
        out.push_str(h.name);
        out.push_str(": ");
        out.push_str(h.value);
        first = false;
    }

    if let Some(token) = bearer_token {
        if !first {
            out.push_str("[0D][0A]");
        }
        out.push_str("Authorization: Bearer ");
        out.push_str(token);
    }

    // 对齐官方工具格式：结尾附加 CRLF。
    if !out.is_empty() {
        out.push_str("[0D][0A]");
    }

    if out.len() > 256 {
        return Err("HTTP 头超过 AT+HTPHD 256 字节限制");
    }

    Ok(out)
}

pub(crate) fn contains_ok(buf: &[u8]) -> bool {
    find_subslice(buf, b"\r\nOK\r\n").is_some()
        || find_subslice(buf, b"\nOK\n").is_some()
        || buf.ends_with(b"OK\r\n")
        || buf.ends_with(b"OK\n")
        || find_subslice(buf, b"FS@HTTP OK:").is_some()
}

pub(crate) fn contains_http_ready(buf: &[u8]) -> bool {
    find_subslice(buf, b"FS@HTTP OK:").is_some()
}

pub(crate) fn contains_http_fail(buf: &[u8]) -> bool {
    find_subslice(buf, b"FS@HTTP FAIL:").is_some()
}

pub(crate) fn contains_at_error(buf: &[u8]) -> bool {
    find_subslice(buf, b"ERR:").is_some() || find_subslice(buf, b"ERROR").is_some()
}

pub(crate) fn parse_status_code(raw: &[u8]) -> Option<u16> {
    for marker in [
        b"FS@HTTP INFO CODE:".as_slice(),
        b"FS@HTTP SUCCESS CODE:".as_slice(),
        b"FS@HTTP REDIRECT CODE:".as_slice(),
        b"FS@HTTP CLIENT ERROR CODE:".as_slice(),
        b"FS@HTTP SERVER ERROR CODE:".as_slice(),
    ] {
        if let Some(code) = parse_fs_http_code(raw, marker) {
            return Some(code);
        }
    }

    if let Some(idx) = find_subslice(raw, b"HTTP/1.") {
        let sub = &raw[idx..];
        if let Some(space) = sub.iter().position(|b| *b == b' ') {
            return parse_u16_from_prefix(&sub[space + 1..]);
        }
    }

    None
}

fn parse_fs_http_code(raw: &[u8], marker: &[u8]) -> Option<u16> {
    let idx = find_subslice(raw, marker)?;
    let sub = &raw[idx..];

    let line_end = sub
        .iter()
        .position(|b| *b == b'\r' || *b == b'\n')
        .unwrap_or(sub.len());
    let line = &sub[..line_end];

    let comma = line.iter().rposition(|b| *b == b',')?;
    parse_u16_from_prefix(&line[comma + 1..])
}

fn parse_u16_from_prefix(data: &[u8]) -> Option<u16> {
    let mut started = false;
    let mut value: u16 = 0;

    for &b in data {
        if b.is_ascii_digit() {
            started = true;
            value = value.saturating_mul(10).saturating_add((b - b'0') as u16);
        } else if started {
            return Some(value);
        }
    }

    if started { Some(value) } else { None }
}
