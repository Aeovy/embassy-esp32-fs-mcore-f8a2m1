# embassy-FS-MCore-F8A2M1

FS-MCore-F8A2M1 (4G Module) DTU AT 驱动程序，专为 Embassy 异步框架和 ESP32 系列设计。

本仓库提供了基于 UART 的异步 HTTP 请求能力，支持在 `no_std` 环境下使用。

## 特性

- **全异步设计**：基于 `embassy` 框架，无缝集成。
- **自定义日志后端**：支持 `defmt` 和 `esp-println`。
- **轻量级**：专为嵌入式环境优化，支持 `alloc`。

## 安装

在 `Cargo.toml` 中引用：

```toml
[dependencies]
embassy-FS-MCore-F8A2M1 = { path = "../patches/esp32-fs-mcore-f8a2m1" }
```

## 日志配置 (Features)

本驱动提供了灵活的日志后端控制，以适应不同的开发环境。通过 `features` 配置，你可以选择适合你的日志输出工具。

### 1. 使用 `defmt` (默认)

如果你使用 `probe-rs` 构建并希望得到紧凑的二进制日志，请使用默认配置或显式启用 `dtu-log-defmt`。

```toml
[dependencies]
embassy-FS-MCore-F8A2M1 = { version = "0.1.0" }
# 或者
embassy-FS-MCore-F8A2M1 = { version = "0.1.0", features = ["dtu-log-defmt"] }
```

### 2. 使用 `esp-println`

如果你在使用普通的串口打印（如 `espflash monitor`）或者不希望引入 `defmt` 复杂性，请关闭默认特性并启用 `dtu-log-esp-println`。

```toml
[dependencies]
embassy-FS-MCore-F8A2M1 = { version = "0.1.0", default-features = false, features = ["dtu-log-esp-println"] }
```

**特别注意**：`dtu-log-defmt` 与 `dtu-log-esp-println` 互斥，同一时间只能启用一个。如果未启用任何一个，编译将报错。

## 快速上手

```rust
use embassy_FS_MCore_F8A2M1::{DtuAtHttpClient, DtuAtHttpConfig, HttpMethod};

// 初始化 UART 并创建客户端
let client = DtuAtHttpClient::new(uart, DtuAtHttpConfig::default());

// 发送 GET 请求
let resp = client.send(HttpMethod::Get, "http://api.example.com/data", &[], &[]).await?;

if resp.is_success() {
    let body = resp.http_body().unwrap_or(&[]);
    println!("Response: {:?}", core::str::from_utf8(body));
}
```

## 贡献

欢迎提交 Issue 和 PR！
