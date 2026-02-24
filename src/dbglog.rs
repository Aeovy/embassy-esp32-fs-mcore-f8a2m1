#[cfg(all(feature = "dtu-log-defmt", feature = "dtu-log-esp-println"))]
compile_error!("日志后端特性冲突：`dtu-log-defmt` 与 `dtu-log-esp-println` 只能启用一个。");

#[cfg(not(any(feature = "dtu-log-defmt", feature = "dtu-log-esp-println")))]
compile_error!("未启用日志后端特性：请启用 `dtu-log-defmt` 或 `dtu-log-esp-println` 之一。");

/// 调试日志输出（仅在 debug 构建启用）。
#[cfg(all(debug_assertions, feature = "dtu-log-defmt"))]
#[inline]
pub(crate) fn emit_debug(msg: &str) {
    defmt::debug!("{}", msg);
}

/// 调试日志输出（仅在 debug 构建启用，esp-println 后端）。
#[cfg(all(debug_assertions, feature = "dtu-log-esp-println"))]
#[inline]
pub(crate) fn emit_debug(msg: &str) {
    esp_println::println!("[DEBUG] {}", msg);
}

/// 调试警告输出（仅在 debug 构建启用）。
#[cfg(all(debug_assertions, feature = "dtu-log-defmt"))]
#[inline]
pub(crate) fn emit_warn(msg: &str) {
    defmt::warn!("{}", msg);
}

/// 调试警告输出（仅在 debug 构建启用，esp-println 后端）。
#[cfg(all(debug_assertions, feature = "dtu-log-esp-println"))]
#[inline]
pub(crate) fn emit_warn(msg: &str) {
    esp_println::println!("[WARN ] {}", msg);
}

/// release 构建下剔除内部调试输出。
#[cfg(not(debug_assertions))]
#[inline]
pub(crate) fn emit_debug(_msg: &str) {}

/// release 构建下剔除内部调试输出。
#[cfg(not(debug_assertions))]
#[inline]
pub(crate) fn emit_warn(_msg: &str) {}

macro_rules! dtu_debug {
    ($($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        {
            let __msg = alloc::format!($($arg)*);
            $crate::dbglog::emit_debug(&__msg);
        }
    }};
}

macro_rules! dtu_warn {
    ($($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        {
            let __msg = alloc::format!($($arg)*);
            $crate::dbglog::emit_warn(&__msg);
        }
    }};
}

pub(crate) use dtu_debug;
pub(crate) use dtu_warn;
