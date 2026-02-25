# embassy-fs-mcore-f8a2m1

FS-MCore-F8A2M1（4G DTU 模块）的异步 AT 指令驱动，基于 Embassy 框架与 `esp-hal`，运行于 ESP32 系列 `no_std` 环境。

通过 UART 向模块发送 AT 指令，完成 HTTP GET / POST 请求的全流程控制。

---

## 目录

- [支持的 ESP 型号](#支持的-esp-型号)
- [引入依赖](#引入依赖)
- [应用项目编译配置](#应用项目编译配置)
- [日志后端配置](#日志后端配置)
- [快速上手](#快速上手)
- [API 参考](#api-参考)
  - [DtuAtHttpClient](#dtuathttpclient)
  - [DtuAtHttpConfig](#dtuathttpconfig)
  - [HttpRequest](#httprequest)
  - [HttpResponse](#httpresponse)
  - [DtuAtError](#dtuaterror)

---

## 支持的 ESP 型号

本库通过 Cargo feature 指定目标芯片型号，并自动透传给底层 `esp-hal`。

| Feature    | 芯片         | 架构     |
|------------|--------------|----------|
| `esp32`    | ESP32        | Xtensa   |
| `esp32s2`  | ESP32-S2     | Xtensa   |
| `esp32s3`  | ESP32-S3     | Xtensa   |
| `esp32c2`  | ESP32-C2     | RISC-V   |
| `esp32c3`  | ESP32-C3     | RISC-V   |
| `esp32c6`  | ESP32-C6     | RISC-V   |
| `esp32h2`  | ESP32-H2     | RISC-V   |

**必须且只能启用一个芯片 feature**，否则编译报错。

---

## 引入依赖

**Step 1**：在应用项目的 `Cargo.toml` 中添加依赖，**必须显式指定芯片型号与日志后端**（本库无 default features）：

```toml
[dependencies]
# 以 ESP32-S3 + defmt 日志为例
embassy-fs-mcore-f8a2m1 = { git = "https://github.com/your-org/esp32-fs-mcore-f8a2m1", features = ["esp32s3", "dtu-log-defmt"] }
```

> 若使用本地路径开发：
> ```toml
> embassy-fs-mcore-f8a2m1 = { path = "/xx/esp32-fs-mcore-f8a2m1", features = ["esp32s3", "dtu-log-defmt"] }
> ```

---

## 应用项目编译配置

**Step 2**：在应用项目根目录创建 `.cargo/config.toml`，设置正确的编译目标。

### Xtensa 系列（ESP32 / ESP32-S2 / ESP32-S3）

```toml
# .cargo/config.toml
[build]
target = "xtensa-esp32s3-none-elf"   # 按实际芯片修改
rustflags = ["-C", "link-arg=-nostartfiles"]

[target.xtensa-esp32s3-none-elf]
runner = "espflash flash --monitor --chip esp32s3 --log-format defmt"
linker = "xtensa-esp32s3-elf-gcc"

[unstable]
build-std = ["alloc", "core"]
```

### RISC-V 系列（ESP32-C2 / C3 / C6 / H2）

使用标准 stable 工具链，无需 `esp` toolchain。

```toml
# .cargo/config.toml
[build]
target = "riscv32imc-unknown-none-elf"   # C2/C3 用此目标
# target = "riscv32imac-unknown-none-elf"  # C6/H2 用此目标
rustflags = ["-C", "link-arg=-nostartfiles"]

[unstable]
build-std = ["alloc", "core"]
```

---

## 日志后端配置

本库内置可选的调试日志，**只在 `debug` 构建输出，`release` 构建零开销**。

两种日志后端互斥，**必须且只能启用一个**：

### 方式一：`defmt`

适合配合 `probe-rs` / `espflash --log-format defmt` 使用。

```toml
embassy-fs-mcore-f8a2m1 = { ..., features = ["esp32s3", "dtu-log-defmt"] }
```

### 方式二：`esp-println`

适合配合 `espflash flash --monitor`（普通串口）使用。

```toml
embassy-fs-mcore-f8a2m1 = { ..., features = ["esp32s3", "dtu-log-esp-println"] }
```

---

## 快速上手

```rust
#![no_std]
#![no_main]

extern crate alloc;

use embassy_executor::Spawner;
use embassy_fs_mcore_f8a2m1::{
    DtuAtHttpClient, DtuAtHttpConfig, HttpHeader, HttpMethod, HttpRequest,
};
use esp_hal::{
    main,
    uart::{Config as UartConfig, Uart},
};

#[main]
async fn main(_spawner: Spawner) {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // 初始化 UART（波特率需与 DTU 模块配置一致）
    let uart = Uart::new(peripherals.UART1, UartConfig::default().with_baudrate(115_200))
        .expect("UART1 init failed")
        .with_rx(peripherals.GPIO16)
        .with_tx(peripherals.GPIO17)
        .into_async();

    let config = DtuAtHttpConfig::default();
    let mut client = DtuAtHttpClient::new(uart, config);

    // --- 简单 GET 请求 ---
    match client
        .send(HttpMethod::Get, "http://httpbin.org/get", &[], &[])
        .await
    {
        Ok(resp) => {
            if resp.is_success() {
                if let Some(body) = resp.http_body() {
                    // 处理响应体
                    let _ = body;
                }
            }
        }
        Err(e) => {
            // 错误处理
            let _ = e.as_str();
        }
    }

    // --- 简单 JSON POST ---
    let payload = b"{\"key\":\"value\"}";
    let _ = client.post_json("http://httpbin.org/post", payload).await;
}
```

---

## API 参考

### DtuAtHttpClient

驱动主入口，持有 UART 传输层与配置。

```rust
pub struct DtuAtHttpClient<'d> { /* ... */ }
```

#### 构造

```rust
pub const fn new(transport: Uart<'d, Async>, config: DtuAtHttpConfig) -> Self
```

#### 核心发送方法

| 方法 | 说明 |
|------|------|
| `send(method, url, headers, body)` | 通用发送接口，支持自定义方法、头、体 |
| `post_json(url, body)` | 快捷 POST JSON，自动追加 `Content-Type: application/json` |
| `request(req)` | 接受完整 [`HttpRequest`](#httprequest) 的底层接口 |

所有发送方法均为 `async`，返回 `Result<HttpResponse, DtuAtError>`。

#### 配置访问

```rust
pub fn config(&self) -> &DtuAtHttpConfig
pub fn config_mut(&mut self) -> &mut DtuAtHttpConfig
```

#### UART 访问

```rust
pub fn transport_mut(&mut self) -> &mut Uart<'d, Async>
pub fn into_transport(self) -> Uart<'d, Async>   // 消费 client，取回 UART
```

---

### DtuAtHttpConfig

控制驱动行为的全部配置，支持 `Default`。

```rust
let config = DtuAtHttpConfig::default();         // 使用默认值
let config = DtuAtHttpConfig { channel: 2, .. DtuAtHttpConfig::default() };
```

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `channel` | `u8` | `1` | DTU HTTP 通道号（1~4） |
| `request_timeout_secs` | `u16` | `10` | AT 层 HTTP 请求超时（秒），对应 `AT+HTPTIM` |
| `response_filter_mask` | `u8` | `0x03` | 响应过滤掩码，对应 `AT+HTPPK` |
| `require_body_on_success` | `bool` | `true` | 2xx 响应时若缺少 body 则返回 `BodyMissing` 错误 |
| `cmd_guard_time` | `Duration` | `200ms` | 发送 `+++` 前的静默保护时间 |
| `at_first_timeout` | `Duration` | `2s` | 等待 AT 响应首字节的超时 |
| `at_idle_timeout` | `Duration` | `250ms` | AT 响应字节间空闲超时（视为结束） |
| `http_first_timeout` | `Duration` | `60s` | 等待 HTTP 响应首字节的超时 |
| `http_idle_timeout` | `Duration` | `300ms` | HTTP 响应字节间空闲超时 |
| `http_followup_first_timeout` | `Duration` | `700ms` | 追加分包等待首字节超时 |
| `http_followup_timeout` | `Duration` | `20s` | 追加分包整体截止时间 |
| `http_ready_timeout` | `Duration` | `25s` | 等待 `FS@HTTP OK` 就绪的总超时 |
| `enable_modem_debug_urc` | `bool` | `false` | 发送前开启模块 URC 调试输出（`AT+DEBUG=ON`） |
| `query_link_status_before_send` | `bool` | `false` | 发送前查询 CREG / RUNST 链路状态（仅调试用） |
| `enable_command_probe_fallback` | `bool` | `false` | `+++` 无响应时自动 fallback 到 `AT` 探测 |
| `retry_payload_on_http_timeout` | `bool` | `false` | HTTP 响应超时后自动重发 payload 一次 |
| `post_entm_settle_time` | `Duration` | `500ms` | `AT+S` 之后、发送 payload 之前的稳定等待 |
| `max_response_len` | `usize` | `4096` | 最大响应缓冲字节数，超出返回 `ResponseTooLarge` |

---

### HttpRequest

完整请求描述，使用 builder 模式构造：

```rust
use embassy_fs_mcore_f8a2m1::{HttpRequest, HttpMethod, HttpHeader, HttpDataType};

let headers = [
    HttpHeader::new("X-App-Key", "my-key"),
    HttpHeader::new("Accept", "application/json"),
];

let req = HttpRequest::new(HttpMethod::Post, "http://api.example.com/data")
    .with_headers(&headers)
    .with_body(b"{\"msg\":\"hello\"}")
    .with_bearer_token("eyJhbGci...")     // 自动追加 Authorization: Bearer <token>
    .with_data_type(HttpDataType::Body);  // Body（默认）或 Query

client.request(&req).await?;
```

| 方法 | 说明 |
|------|------|
| `new(method, url)` | 创建请求（最小输入） |
| `with_headers(headers)` | 设置请求头列表 |
| `with_body(body)` | 设置请求体 |
| `with_bearer_token(token)` | 设置 Bearer Token（追加 `Authorization` 头） |
| `with_data_type(dt)` | 设置 DTU 数据类型（`Body` / `Query`） |

---

### HttpResponse

```rust
pub struct HttpResponse {
    pub status_code: Option<u16>,  // HTTP 状态码，解析失败时为 None
    pub raw: Vec<u8>,              // 模块原始响应字节
}
```

| 方法 | 说明 |
|------|------|
| `is_success()` | status_code 在 200~299 范围内时返回 `true` |
| `http_body()` | 尝试从原始响应中提取 HTTP body，返回 `Option<&[u8]>` |
| `declared_content_length()` | 解析 HTTP 头中声明的 `Content-Length` |
| `as_utf8_lossy()` | 将 `raw` 按 UTF-8 宽松解码为 `String` |

---

### DtuAtError

```rust
pub enum DtuAtError {
    Transport(IoError),         // UART 底层 IO 错误
    Timeout,                    // 等待响应超时
    WriteZero,                  // UART 写入返回 0 字节
    InvalidConfig(&'static str),// 配置参数不合法
    AtRejected,                 // 模块回复 ERR / ERROR
    BadResponse,                // 响应中未看到 OK
    ResponseTooLarge,           // 响应超过 max_response_len
    BodyMissing,                // 2xx 响应成功但缺少 body
}
```

所有变体均可通过 `.as_str()` 获取静态描述字符串，方便 `defmt` / `esp-println` 输出：

```rust
if let Err(e) = client.send(...).await {
    defmt::error!("HTTP error: {}", e.as_str());
}
```

// 初始化 UART 并创建客户端
let client = DtuAtHttpClient::new(uart, DtuAtHttpConfig::default());

// 发送 GET 请求
let resp = client.send(HttpMethod::Get, "http://api.example.com/data", &[], &[]).await?;

if resp.is_success() {
    let body = resp.http_body().unwrap_or(&[]);
    println!("Response: {:?}", core::str::from_utf8(body));
}
```
