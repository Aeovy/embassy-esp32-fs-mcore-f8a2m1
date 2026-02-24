use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use core::fmt::Write as _;
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embedded_io_async::{Read as AsyncRead, Write as AsyncWrite};
use esp_hal::{Async, uart::Uart};

use crate::dbglog::{dtu_debug, dtu_warn};
use crate::parser::{
    build_head_line, contains_at_error, contains_http_fail, contains_http_ready, contains_ok,
    parse_status_code,
};
use crate::types::{DtuAtError, DtuAtHttpConfig, HttpRequest, HttpResponse};

/// DTU 异步 HTTP 客户端（UART 驱动）。
///
/// 底层固定使用 `esp_hal::uart::Uart<'d, Async>`。
pub struct DtuAtHttpClient<'d> {
    transport: Uart<'d, Async>,
    config: DtuAtHttpConfig,
}

impl<'d> DtuAtHttpClient<'d> {
    /// 创建客户端。
    ///
    /// # 输入
    /// - `transport`: 已初始化的异步 UART。
    /// - `config`: 驱动配置。
    ///
    /// # 返回
    /// 返回可用于发送 HTTP 请求的客户端实例。
    pub const fn new(transport: Uart<'d, Async>, config: DtuAtHttpConfig) -> Self {
        Self { transport, config }
    }

    /// 获取当前配置（只读）。
    pub fn config(&self) -> &DtuAtHttpConfig {
        &self.config
    }

    /// 获取当前配置（可写）。
    pub fn config_mut(&mut self) -> &mut DtuAtHttpConfig {
        &mut self.config
    }

    /// 访问底层 UART（可写）。
    pub fn transport_mut(&mut self) -> &mut Uart<'d, Async> {
        &mut self.transport
    }

    /// 取回底层 UART，消费客户端实例。
    pub fn into_transport(self) -> Uart<'d, Async> {
        self.transport
    }

    /// 最简发送接口。
    ///
    /// # 输入
    /// - `method`: 请求方法。
    /// - `url`: 完整 URL。
    /// - `headers`: 请求头列表。
    /// - `body`: 请求体字节。
    ///
    /// # 返回
    /// - `Ok(HttpResponse)`: 请求完成并得到响应（可通过 `status_code` 判断 HTTP 结果）。
    /// - `Err(DtuAtError)`: 发送/等待/解析过程中出错。
    pub async fn send(
        &mut self,
        method: crate::types::HttpMethod,
        url: &str,
        headers: &[crate::types::HttpHeader<'_>],
        body: &[u8],
    ) -> Result<HttpResponse, DtuAtError> {
        let req = HttpRequest::new(method, url)
            .with_headers(headers)
            .with_data_type(crate::types::HttpDataType::Body)
            .with_body(body);
        self.request(&req).await
    }

    /// 最简 JSON POST 接口。
    ///
    /// 自动追加：
    /// - `Content-Type: application/json`
    pub async fn post_json(&mut self, url: &str, body: &[u8]) -> Result<HttpResponse, DtuAtError> {
        let headers = [crate::types::HttpHeader::new(
            "Content-Type",
            "application/json",
        )];
        self.send(crate::types::HttpMethod::Post, url, &headers, body)
            .await
    }

    /// 完整请求接口。
    ///
    /// # 输入
    /// - `req`: 完整请求结构（方法、URL、Header、Body 等）。
    ///
    /// # 成功
    /// 返回 [`HttpResponse`]，其中 `status_code` 可能为 `Some(200)` 等。
    ///
    /// # 错误
    /// 返回 [`DtuAtError`]，例如超时、AT 拒绝、响应格式不合法等。
    pub async fn request(&mut self, req: &HttpRequest<'_>) -> Result<HttpResponse, DtuAtError> {
        dtu_debug!(
            "dtu_http request start, ch={}, method={}, url={}",
            self.config.channel,
            req.method.as_at(),
            req.url
        );
        self.validate_request(req)?;

        self.enter_command_mode().await.map_err(|e| {
            dtu_warn!("dtu_http step=enter_command_mode failed: {}", e.as_str());
            e
        })?;
        self.send_ok_cmd(&format!("AT+WKMOD{}=HTTP", self.config.channel))
            .await
            .map_err(|e| {
                dtu_warn!("dtu_http step=WKMOD failed: {}", e.as_str());
                e
            })?;
        self.send_ok_cmd(&format!(
            "AT+HTPTP{}={}",
            self.config.channel,
            req.method.as_at()
        ))
        .await
        .map_err(|e| {
            dtu_warn!("dtu_http step=HTPTP failed: {}", e.as_str());
            e
        })?;
        self.send_ok_cmd(&format!("AT+HTPURL{}={}", self.config.channel, req.url))
            .await
            .map_err(|e| {
                dtu_warn!("dtu_http step=HTPURL failed: {}", e.as_str());
                e
            })?;

        let head_line =
            build_head_line(req.headers, req.bearer_token).map_err(DtuAtError::InvalidConfig)?;
        dtu_debug!("dtu_http headers prepared, len={}", head_line.len());
        if !head_line.is_empty() {
            self.send_ok_cmd(&format!("AT+HTPHD{}={}", self.config.channel, head_line))
                .await
                .map_err(|e| {
                    dtu_warn!("dtu_http step=HTPHD failed: {}", e.as_str());
                    e
                })?;
        }

        self.send_ok_cmd(&format!(
            "AT+HTPPK{}={}",
            self.config.channel, self.config.response_filter_mask
        ))
        .await
        .map_err(|e| {
            dtu_warn!("dtu_http step=HTPPK failed: {}", e.as_str());
            e
        })?;
        self.send_ok_cmd(&format!(
            "AT+HTPTIM{}={}",
            self.config.channel, self.config.request_timeout_secs
        ))
        .await
        .map_err(|e| {
            dtu_warn!("dtu_http step=HTPTIM failed: {}", e.as_str());
            e
        })?;
        self.send_ok_cmd(&format!(
            "AT+HTPDT{}={}",
            self.config.channel,
            req.data_type.as_at()
        ))
        .await
        .map_err(|e| {
            dtu_warn!("dtu_http step=HTPDT failed: {}", e.as_str());
            e
        })?;

        if self.config.enable_modem_debug_urc {
            if let Err(e) = self.send_ok_cmd("AT+DEBUG=ON").await {
                dtu_warn!("dtu_http step=DEBUG_ON failed (continue): {}", e.as_str());
            }
        }

        if self.config.query_link_status_before_send {
            self.try_log_link_status().await;
        }

        self.send_save_and_wait_http_ready().await.map_err(|e| {
            dtu_warn!(
                "dtu_http step=save_reboot_wait_ready failed: {}",
                e.as_str()
            );
            e
        })?;

        dtu_debug!(
            "dtu_http wait post_ready_settle={}ms",
            self.config.post_entm_settle_time.as_millis()
        );
        Timer::after(self.config.post_entm_settle_time).await;

        self.send_payload(req.body).await?;

        let raw = match self
            .read_until_idle(
                self.config.http_first_timeout,
                self.config.http_idle_timeout,
            )
            .await
        {
            Ok(raw) => raw,
            Err(DtuAtError::Timeout) if self.config.retry_payload_on_http_timeout => {
                dtu_warn!("dtu_http step=read_http_response timeout, retry payload once");
                self.send_payload(req.body).await?;
                self.read_until_idle(
                    self.config.http_first_timeout,
                    self.config.http_idle_timeout,
                )
                .await
                .map_err(|e| {
                    dtu_warn!(
                        "dtu_http step=read_http_response_retry failed: {}",
                        e.as_str()
                    );
                    e
                })?
            }
            Err(e) => {
                dtu_warn!("dtu_http step=read_http_response failed: {}", e.as_str());
                return Err(e);
            }
        };

        let raw = self.collect_followup_http_data(raw).await?;
        log_response_preview("http", &raw);

        let resp = HttpResponse {
            status_code: parse_status_code(&raw),
            raw,
        };

        let allow_empty_body = matches!(resp.status_code, Some(204 | 304));

        if self.config.require_body_on_success && resp.is_success() && !allow_empty_body {
            let body_missing = match resp.http_body() {
                Some(body) => body.is_empty(),
                None => true,
            };

            if body_missing {
                if let Some(content_len) = resp.declared_content_length() {
                    dtu_warn!(
                        "dtu_http success but body missing, declared content-length={}",
                        content_len
                    );
                } else {
                    dtu_warn!("dtu_http success but body missing (no content-length found)");
                }
                return Err(DtuAtError::BodyMissing);
            }
        }

        Ok(resp)
    }

    async fn collect_followup_http_data(
        &mut self,
        mut raw: Vec<u8>,
    ) -> Result<Vec<u8>, DtuAtError> {
        let deadline = Instant::now() + self.config.http_followup_timeout;
        let mut timeout_streak = 0u8;
        let mut appended = false;
        let mut got_non_urc_payload = false;

        while Instant::now() < deadline {
            let poll_first_timeout = short_poll_timeout(self.config.http_followup_first_timeout);
            let chunk = match self
                .read_until_idle_quiet(poll_first_timeout, self.config.http_idle_timeout)
                .await
            {
                Ok(c) => c,
                Err(DtuAtError::Timeout) => {
                    timeout_streak = timeout_streak.saturating_add(1);
                    if timeout_streak >= 1 && (appended || got_non_urc_payload) {
                        break;
                    }
                    continue;
                }
                Err(e) => return Err(e),
            };

            if chunk.is_empty() {
                timeout_streak = timeout_streak.saturating_add(1);
                if timeout_streak >= 1 && (appended || got_non_urc_payload) {
                    break;
                }
                continue;
            }

            timeout_streak = 0;
            appended = true;
            let non_urc = !chunk.starts_with(b"FS@");
            if non_urc {
                got_non_urc_payload = true;
            }
            log_response_preview("http_followup", &chunk);

            if raw.len() + chunk.len() > self.config.max_response_len {
                return Err(DtuAtError::ResponseTooLarge);
            }
            raw.extend_from_slice(&chunk);

            if non_urc {
                break;
            }
        }

        Ok(raw)
    }

    async fn send_payload(&mut self, payload: &[u8]) -> Result<(), DtuAtError> {
        dtu_debug!("dtu_http payload bytes={}", payload.len());
        if payload.is_empty() {
            self.write_all(b" ").await.map_err(|e| {
                dtu_warn!("dtu_http step=send_payload(empty) failed: {}", e.as_str());
                e
            })
        } else {
            self.write_all(payload).await.map_err(|e| {
                dtu_warn!("dtu_http step=send_payload failed: {}", e.as_str());
                e
            })
        }
    }

    async fn send_save_and_wait_http_ready(&mut self) -> Result<(), DtuAtError> {
        dtu_debug!("dtu_http >> AT+S");
        self.write_all(b"AT+S").await?;
        self.write_all(b"\r\n").await?;

        let deadline = Instant::now() + self.config.http_ready_timeout;
        let mut merged = Vec::new();

        while Instant::now() < deadline {
            let poll_first_timeout = short_poll_timeout(self.config.at_first_timeout);
            let chunk = match self
                .read_until_idle_quiet(poll_first_timeout, self.config.at_idle_timeout)
                .await
            {
                Ok(c) => c,
                Err(DtuAtError::Timeout) => {
                    continue;
                }
                Err(e) => return Err(e),
            };

            if chunk.is_empty() {
                continue;
            }

            log_response_preview("save_wait", &chunk);

            if merged.len() + chunk.len() <= self.config.max_response_len {
                merged.extend_from_slice(&chunk);
            }

            if contains_at_error(&merged) {
                return Err(DtuAtError::AtRejected);
            }
            if contains_http_fail(&merged) {
                return Err(DtuAtError::BadResponse);
            }
            if contains_http_ready(&merged) {
                dtu_debug!("dtu_http got FS@HTTP OK, ready for payload");
                return Ok(());
            }
        }

        dtu_warn!("dtu_http wait FS@HTTP OK timeout");
        log_response_preview("save_wait_final", &merged);
        Err(DtuAtError::Timeout)
    }

    async fn try_log_link_status(&mut self) {
        match self.send_query_cmd("AT+CREG?").await {
            Ok(rsp) => log_response_preview("creg", &rsp),
            Err(e) => dtu_warn!("dtu_http link query CREG failed: {}", e.as_str()),
        }

        match self.send_query_cmd("AT+RUNST?").await {
            Ok(rsp) => log_response_preview("runst", &rsp),
            Err(e) => dtu_warn!("dtu_http link query RUNST failed: {}", e.as_str()),
        }
    }

    fn validate_request(&self, req: &HttpRequest<'_>) -> Result<(), DtuAtError> {
        if !(1..=4).contains(&self.config.channel) {
            return Err(DtuAtError::InvalidConfig("channel 必须在 1~4"));
        }
        if req.url.is_empty() {
            return Err(DtuAtError::InvalidConfig("url 不能为空"));
        }
        if self.config.max_response_len == 0 {
            return Err(DtuAtError::InvalidConfig("max_response_len 不能为 0"));
        }
        Ok(())
    }

    async fn enter_command_mode(&mut self) -> Result<(), DtuAtError> {
        dtu_debug!("dtu_http enter command mode via +++");
        Timer::after(self.config.cmd_guard_time).await;
        self.write_all(b"+++").await?;

        match self
            .read_until_idle(self.config.at_first_timeout, self.config.at_idle_timeout)
            .await
        {
            Ok(rsp) => {
                log_response_preview("enter_cmd", &rsp);

                if contains_ok(&rsp) {
                    return Ok(());
                }

                if self.config.enable_command_probe_fallback {
                    if contains_at_error(&rsp) {
                        dtu_warn!("dtu_http +++ got ERR/ERROR, fallback to AT probe");
                    } else {
                        dtu_warn!("dtu_http +++ no OK, fallback to AT probe");
                    }
                    return self.probe_command_mode().await;
                }

                return Err(DtuAtError::BadResponse);
            }
            Err(DtuAtError::Timeout) => {
                if self.config.enable_command_probe_fallback {
                    dtu_warn!("dtu_http +++ timeout, fallback to AT probe");
                    return self.probe_command_mode().await;
                }
                return Err(DtuAtError::Timeout);
            }
            Err(e) => return Err(e),
        }
    }

    async fn probe_command_mode(&mut self) -> Result<(), DtuAtError> {
        dtu_debug!("dtu_http >> AT (probe)");
        self.write_all(b"AT\r\n").await?;

        let rsp = self
            .read_until_idle(Duration::from_secs(2), self.config.at_idle_timeout)
            .await?;
        log_response_preview("probe_cmd", &rsp);

        if contains_at_error(&rsp) {
            return Err(DtuAtError::AtRejected);
        }
        if !contains_ok(&rsp) {
            return Err(DtuAtError::BadResponse);
        }

        dtu_debug!("dtu_http probe success, treat as command mode");
        Ok(())
    }

    async fn send_ok_cmd(&mut self, cmd: &str) -> Result<(), DtuAtError> {
        dtu_debug!("dtu_http >> {}", cmd);
        self.write_all(cmd.as_bytes()).await?;
        self.write_all(b"\r\n").await?;

        let rsp = self
            .read_until_idle(self.config.at_first_timeout, self.config.at_idle_timeout)
            .await?;

        log_response_preview("at_rsp", &rsp);

        if contains_at_error(&rsp) {
            return Err(DtuAtError::AtRejected);
        }
        if !contains_ok(&rsp) {
            return Err(DtuAtError::BadResponse);
        }
        Ok(())
    }

    async fn send_query_cmd(&mut self, cmd: &str) -> Result<Vec<u8>, DtuAtError> {
        dtu_debug!("dtu_http >> {}", cmd);
        self.write_all(cmd.as_bytes()).await?;
        self.write_all(b"\r\n").await?;

        let rsp = self
            .read_until_idle(self.config.at_first_timeout, self.config.at_idle_timeout)
            .await?;

        log_response_preview("query_rsp", &rsp);

        if contains_at_error(&rsp) {
            return Err(DtuAtError::AtRejected);
        }
        if !contains_ok(&rsp) {
            return Err(DtuAtError::BadResponse);
        }

        Ok(rsp)
    }

    async fn write_all(&mut self, mut buf: &[u8]) -> Result<(), DtuAtError> {
        while !buf.is_empty() {
            let written = AsyncWrite::write(&mut self.transport, buf)
                .await
                .map_err(DtuAtError::Transport)?;

            if written == 0 {
                return Err(DtuAtError::WriteZero);
            }
            buf = &buf[written..];
        }

        AsyncWrite::flush(&mut self.transport)
            .await
            .map_err(DtuAtError::Transport)?;
        Ok(())
    }

    async fn read_until_idle(
        &mut self,
        first_timeout: Duration,
        idle_timeout: Duration,
    ) -> Result<Vec<u8>, DtuAtError> {
        self.read_until_idle_impl(first_timeout, idle_timeout, true)
            .await
    }

    async fn read_until_idle_quiet(
        &mut self,
        first_timeout: Duration,
        idle_timeout: Duration,
    ) -> Result<Vec<u8>, DtuAtError> {
        self.read_until_idle_impl(first_timeout, idle_timeout, false)
            .await
    }

    async fn read_until_idle_impl(
        &mut self,
        first_timeout: Duration,
        idle_timeout: Duration,
        log_first_timeout: bool,
    ) -> Result<Vec<u8>, DtuAtError> {
        let mut out = Vec::new();
        let mut chunk = [0u8; 256];
        let mut got_any = false;

        loop {
            let timeout = if got_any { idle_timeout } else { first_timeout };
            let read_result =
                with_timeout(timeout, AsyncRead::read(&mut self.transport, &mut chunk)).await;

            let n = match read_result {
                Ok(result) => result.map_err(DtuAtError::Transport)?,
                Err(_) => {
                    if got_any {
                        dtu_debug!(
                            "dtu_http read idle timeout after receiving bytes, stop collecting"
                        );
                        break;
                    }
                    if log_first_timeout {
                        dtu_warn!("dtu_http read first byte timeout");
                    }
                    return Err(DtuAtError::Timeout);
                }
            };

            if n == 0 {
                break;
            }

            got_any = true;
            if out.len() + n > self.config.max_response_len {
                return Err(DtuAtError::ResponseTooLarge);
            }
            out.extend_from_slice(&chunk[..n]);
        }

        Ok(out)
    }
}

fn short_poll_timeout(base: Duration) -> Duration {
    if base.as_millis() > 800 {
        Duration::from_millis(800)
    } else {
        base
    }
}

fn log_response_preview(tag: &'static str, buf: &[u8]) {
    let preview_len = core::cmp::min(160, buf.len());
    let preview = &buf[..preview_len];

    if let Ok(text) = core::str::from_utf8(preview) {
        dtu_debug!("dtu_http {} rsp_len={}, preview={}", tag, buf.len(), text);
    } else {
        let hex = bytes_to_hex(preview);
        dtu_debug!(
            "dtu_http {} rsp_len={}, preview_hex={}",
            tag,
            buf.len(),
            hex
        );
    }
}

fn bytes_to_hex(data: &[u8]) -> String {
    let mut out = String::new();
    for (idx, b) in data.iter().enumerate() {
        if idx > 0 {
            let _ = out.write_str(" ");
        }
        let _ = write!(&mut out, "{:02X}", b);
    }
    out
}
