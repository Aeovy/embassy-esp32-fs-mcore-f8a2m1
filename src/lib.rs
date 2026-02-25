#![no_std]

// ── feature 合法性校验 ─────────────────────────────────────────────────────
#[cfg(not(any(
    feature = "esp32",
    feature = "esp32s2",
    feature = "esp32s3",
    feature = "esp32c2",
    feature = "esp32c3",
    feature = "esp32c6",
    feature = "esp32h2",
)))]
compile_error!(
    "请启用一个芯片 feature：esp32 / esp32s2 / esp32s3 / esp32c2 / esp32c3 / esp32c6 / esp32h2"
);

extern crate alloc;

mod client;
#[macro_use]
pub(crate) mod dbglog;
mod parser;
mod types;
mod util;

pub use client::DtuAtHttpClient;
pub use types::{
    DtuAtError, DtuAtHttpConfig, HttpDataType, HttpHeader, HttpMethod, HttpRequest, HttpResponse,
};
