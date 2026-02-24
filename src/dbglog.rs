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
#[allow(dead_code)]
#[inline]
pub(crate) fn emit_debug(_msg: &str) {}

/// release 构建下剔除内部调试输出。
#[cfg(not(debug_assertions))]
#[allow(dead_code)]
#[inline]
pub(crate) fn emit_warn(_msg: &str) {}

// debug 构建：正常输出日志
#[cfg(debug_assertions)]
macro_rules! dtu_debug {
    ($($arg:tt)*) => {{
        let __msg = alloc::format!($($arg)*);
        $crate::dbglog::emit_debug(&__msg);
    }};
}

// release 构建：`if false` 让编译器认为变量已使用，优化器直接死代码消除
#[cfg(not(debug_assertions))]
macro_rules! dtu_debug {
    ($($arg:tt)*) => {
        if false {
            let _ = alloc::format!($($arg)*);
        }
    };
}

// debug 构建：正常输出警告
#[cfg(debug_assertions)]
macro_rules! dtu_warn {
    ($($arg:tt)*) => {{
        let __msg = alloc::format!($($arg)*);
        $crate::dbglog::emit_warn(&__msg);
    }};
}

// release 构建：同上，仅消除 unused variable 警告
#[cfg(not(debug_assertions))]
macro_rules! dtu_warn {
    ($($arg:tt)*) => {
        if false {
            let _ = alloc::format!($($arg)*);
        }
    };
}

pub(crate) use dtu_debug;
pub(crate) use dtu_warn;
