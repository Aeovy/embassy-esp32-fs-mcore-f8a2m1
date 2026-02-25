use alloc::string::String;
use alloc::vec::Vec;

use embassy_time::Duration;
use esp_hal::uart::IoError;

use crate::util::find_subslice;

/// HTTP 请求方法（DTU 指令集仅支持 GET / POST）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    pub(crate) fn as_at(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

/// 串口数据类型，对应 `AT+HTPDT`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpDataType {
    Body,
    Query,
}

impl HttpDataType {
    pub(crate) fn as_at(self) -> &'static str {
        match self {
            Self::Body => "BODY",
            Self::Query => "QUERY",
        }
    }
}

/// HTTP 头键值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpHeader<'a> {
    pub name: &'a str,
    pub value: &'a str,
}

impl<'a> HttpHeader<'a> {
    /// 构造一个请求头。
    ///
    /// # 参数
    /// - `name`: Header 名称，如 `"Content-Type"`。
    /// - `value`: Header 值，如 `"application/json"`。
    pub const fn new(name: &'a str, value: &'a str) -> Self {
        Self { name, value }
    }
}

/// 面向业务层的 HTTP 请求模型。
#[derive(Debug, Clone, Copy)]
pub struct HttpRequest<'a> {
    pub method: HttpMethod,
    pub url: &'a str,
    pub headers: &'a [HttpHeader<'a>],
    pub body: &'a [u8],
    pub bearer_token: Option<&'a str>,
    pub data_type: HttpDataType,
}

impl<'a> HttpRequest<'a> {
    /// 创建请求（最小输入：`method + url`）。
    pub const fn new(method: HttpMethod, url: &'a str) -> Self {
        Self {
            method,
            url,
            headers: &[],
            body: &[],
            bearer_token: None,
            data_type: HttpDataType::Body,
        }
    }

    /// 设置请求头列表。
    pub const fn with_headers(mut self, headers: &'a [HttpHeader<'a>]) -> Self {
        self.headers = headers;
        self
    }

    /// 设置请求体。
    pub const fn with_body(mut self, body: &'a [u8]) -> Self {
        self.body = body;
        self
    }

    /// 设置 Bearer Token（会拼接为 Authorization 头）。
    pub const fn with_bearer_token(mut self, token: &'a str) -> Self {
        self.bearer_token = Some(token);
        self
    }

    /// 设置 DTU 串口数据类型。
    pub const fn with_data_type(mut self, data_type: HttpDataType) -> Self {
        self.data_type = data_type;
        self
    }
}

/// HTTP 响应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status_code: Option<u16>,
    pub raw: Vec<u8>,
}

impl HttpResponse {
    /// 是否为成功响应。
    pub fn is_success(&self) -> bool {
        matches!(self.status_code, Some(200..=299))
    }

    /// 将原始响应按 UTF-8 宽松解码为字符串。
    pub fn as_utf8_lossy(&self) -> String {
        String::from_utf8_lossy(&self.raw).into_owned()
    }

    /// 尝试提取 HTTP body。
    pub fn http_body(&self) -> Option<&[u8]> {
        let raw = self.raw.as_slice();

        if let Some(http_idx) = find_subslice(raw, b"HTTP/1.") {
            let http = &raw[http_idx..];

            if let Some((header_end, sep_len)) = find_header_boundary(http) {
                let body_start = http_idx + header_end + sep_len;
                let body = &raw[body_start..];

                if body.starts_with(b"FS@") {
                    return None;
                }

                if let Some(content_len) = parse_content_length(&http[..header_end]) {
                    if content_len == 0 {
                        return Some(&[]);
                    }

                    if body.len() >= content_len {
                        return Some(&body[..content_len]);
                    }

                    return Some(body);
                }

                if body.is_empty() {
                    return None;
                }

                return Some(body);
            }
        }

        if let Some(idx) = find_subslice(raw, b"\r\n\r\n") {
            let body = &raw[idx + 4..];
            if body.starts_with(b"FS@") {
                return None;
            }
            return Some(body);
        }

        if let Some(idx) = find_subslice(raw, b"\n\n") {
            let body = &raw[idx + 2..];
            if body.starts_with(b"FS@") {
                return None;
            }
            return Some(body);
        }

        if let Some(body) = extract_urc_style_body(raw) {
            return Some(body);
        }

        None
    }

    /// 从 HTTP 头中解析声明的 `Content-Length`。
    pub fn declared_content_length(&self) -> Option<usize> {
        let raw = self.raw.as_slice();
        let http_idx = find_subslice(raw, b"HTTP/1.")?;
        let http = &raw[http_idx..];
        let (header_end, _) = find_header_boundary(http)?;
        parse_content_length(&http[..header_end])
    }
}

fn find_header_boundary(http: &[u8]) -> Option<(usize, usize)> {
    if let Some(idx) = find_subslice(http, b"\r\n\r\n") {
        return Some((idx, 4));
    }
    if let Some(idx) = find_subslice(http, b"\n\n") {
        return Some((idx, 2));
    }
    None
}

fn extract_urc_style_body(raw: &[u8]) -> Option<&[u8]> {
    for marker in [
        b"FS@HTTP INFO CODE:".as_slice(),
        b"FS@HTTP SUCCESS CODE:".as_slice(),
        b"FS@HTTP REDIRECT CODE:".as_slice(),
        b"FS@HTTP CLIENT ERROR CODE:".as_slice(),
        b"FS@HTTP SERVER ERROR CODE:".as_slice(),
    ] {
        let Some(idx) = find_subslice(raw, marker) else {
            continue;
        };

        let mut pos = idx + marker.len();

        let Some(comma_rel) = raw[pos..].iter().position(|b| *b == b',') else {
            continue;
        };
        pos += comma_rel + 1;

        while pos < raw.len() && raw[pos].is_ascii_digit() {
            pos += 1;
        }
        while pos < raw.len()
            && (raw[pos] == b' ' || raw[pos] == b'\t' || raw[pos] == b'\r' || raw[pos] == b'\n')
        {
            pos += 1;
        }

        if pos >= raw.len() {
            continue;
        }

        let mut end = raw.len();
        if let Some(rel) = find_subslice(&raw[pos..], b"\r\nFS@") {
            end = pos + rel;
        } else if let Some(rel) = find_subslice(&raw[pos..], b"\nFS@") {
            end = pos + rel;
        } else if pos + 1 < raw.len() {
            if let Some(rel) = find_subslice(&raw[pos + 1..], b"FS@") {
                end = pos + 1 + rel;
            }
        }

        let body = trim_ascii_whitespace(&raw[pos..end]);
        if body.is_empty() || body.starts_with(b"FS@") {
            continue;
        }
        return Some(body);
    }

    None
}

fn trim_ascii_whitespace(data: &[u8]) -> &[u8] {
    let mut start = 0usize;
    let mut end = data.len();

    while start < end && data[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && data[end - 1].is_ascii_whitespace() {
        end -= 1;
    }

    &data[start..end]
}

fn parse_content_length(header: &[u8]) -> Option<usize> {
    let mut i = 0usize;
    while i < header.len() {
        let line_end = header[i..]
            .iter()
            .position(|b| *b == b'\n')
            .map(|p| i + p)
            .unwrap_or(header.len());

        let line = &header[i..line_end];
        if line.len() >= 15 {
            let prefix = b"Content-Length:";
            if line.len() >= prefix.len() && eq_ascii_case_prefix(line, prefix) {
                let mut j = prefix.len();
                while j < line.len() && (line[j] == b' ' || line[j] == b'\t') {
                    j += 1;
                }
                return parse_usize_from_prefix(&line[j..]);
            }
        }

        i = if line_end < header.len() {
            line_end + 1
        } else {
            header.len()
        };
    }
    None
}

fn parse_usize_from_prefix(data: &[u8]) -> Option<usize> {
    let mut started = false;
    let mut value: usize = 0;
    for &b in data {
        if b.is_ascii_digit() {
            started = true;
            value = value.saturating_mul(10).saturating_add((b - b'0') as usize);
        } else if started {
            return Some(value);
        }
    }
    if started { Some(value) } else { None }
}

fn eq_ascii_case_prefix(line: &[u8], prefix: &[u8]) -> bool {
    if line.len() < prefix.len() {
        return false;
    }
    for (a, b) in line[..prefix.len()].iter().zip(prefix.iter()) {
        if a.to_ascii_lowercase() != b.to_ascii_lowercase() {
            return false;
        }
    }
    true
}

/// DTU HTTP 客户端配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DtuAtHttpConfig {
    pub channel: u8,
    pub request_timeout_secs: u16,
    pub response_filter_mask: u8,
    pub require_body_on_success: bool,
    pub cmd_guard_time: Duration,
    pub at_first_timeout: Duration,
    pub at_idle_timeout: Duration,
    pub http_first_timeout: Duration,
    pub http_idle_timeout: Duration,
    pub http_followup_first_timeout: Duration,
    pub http_followup_timeout: Duration,
    pub http_ready_timeout: Duration,
    pub enable_modem_debug_urc: bool,
    pub query_link_status_before_send: bool,
    pub enable_command_probe_fallback: bool,
    pub retry_payload_on_http_timeout: bool,
    pub post_entm_settle_time: Duration,
    pub max_response_len: usize,
    /// AT+S 后 DTU 会重启，probe_command_mode 重试等待 DTU 上线的总超时时间。
    /// 建议设置为 DTU 重启时间的 2 倍，默认 60s。
    pub at_ready_timeout: Duration,
    /// 每次 AT\r\n 探测失败后的等待间隔，默认 2s。
    pub at_ready_poll_interval: Duration,
    /// 进入命令模式时先发 AT 探测（而非直接 +++），用于 AT+S 重启后 DTU 已在命令模式的场景。
    pub probe_cmd_mode_first: bool,
}

impl Default for DtuAtHttpConfig {
    fn default() -> Self {
        Self {
            channel: 1,
            request_timeout_secs: 10,
            response_filter_mask: 0x03,
            require_body_on_success: true,
            cmd_guard_time: Duration::from_millis(200),
            at_first_timeout: Duration::from_secs(2),
            at_idle_timeout: Duration::from_millis(250),
            http_first_timeout: Duration::from_secs(60),
            http_idle_timeout: Duration::from_millis(300),
            http_followup_first_timeout: Duration::from_millis(700),
            http_followup_timeout: Duration::from_secs(20),
            http_ready_timeout: Duration::from_secs(25),
            enable_modem_debug_urc: false,
            query_link_status_before_send: false,
            enable_command_probe_fallback: false,
            retry_payload_on_http_timeout: false,
            post_entm_settle_time: Duration::from_millis(500),
            max_response_len: 4096,
            at_ready_timeout: Duration::from_secs(60),
            at_ready_poll_interval: Duration::from_secs(2),
            probe_cmd_mode_first: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DtuAtError {
    Transport(IoError),
    Timeout,
    WriteZero,
    InvalidConfig(&'static str),
    AtRejected,
    BadResponse,
    ResponseTooLarge,
    BodyMissing,
    /// DTU 固件级 HTTP 失败（FS@HTTP FAIL:N），通常为 TLS/连接层错误。
    /// 携带 DTU 返回的错误码（0 表示未解析到）。
    HttpFail(u8),
}

impl DtuAtError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Transport(_) => "transport error",
            Self::Timeout => "timeout",
            Self::WriteZero => "write returned zero",
            Self::InvalidConfig(msg) => msg,
            Self::AtRejected => "AT rejected (ERR/ERROR)",
            Self::BadResponse => "AT response missing OK",
            Self::ResponseTooLarge => "response too large",
            Self::BodyMissing => "http body missing",
            Self::HttpFail(_) => "DTU HTTP FAIL (TLS/connection error)",
        }
    }
}
