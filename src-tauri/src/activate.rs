//! 点击宠物时激活 ZCode 应用到前台；检测 ZCode 是否在前台。

/// ZCode 的 bundle identifier（用于前台检测）。
const ZCODE_BUNDLE_ID: &str = "dev.zcode.app";

#[tauri::command]
pub fn activate_target() {
    // 用 LaunchServices 重新打开而非 osascript activate：
    // ZCode 红绿灯关闭后进程仍在（Electron 在 macOS 不随窗口关闭退出），
    // 普通 activate 只前置无窗口的应用，不会恢复主窗口；
    // open -b 走 reopen 语义，会触发 ZCode 的 activate 处理恢复窗口，
    // 且不需要 Apple Events 自动化授权。
    let _ = std::process::Command::new("open")
        .args(["-b", ZCODE_BUNDLE_ID])
        .output();
}

/// 检测 ZCode 是否当前台应用（用户正在 ZCode 窗口）。
///
/// 权限请求时若用户已在 ZCode，只需气泡提示，无需弹确认窗。
#[tauri::command]
pub fn is_zcode_frontmost() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::runtime::Object;
        let ws: *mut Object = msg_send![objc::class!(NSWorkspace), sharedWorkspace];
        if ws.is_null() {
            return false;
        }
        let front: *mut Object = msg_send![ws, frontmostApplication];
        if front.is_null() {
            return false;
        }
        let bid: *mut Object = msg_send![front, bundleIdentifier];
        if bid.is_null() {
            return false;
        }
        let cstr: *const std::os::raw::c_char = msg_send![bid, UTF8String];
        if cstr.is_null() {
            return false;
        }
        std::ffi::CStr::from_ptr(cstr)
            .to_str()
            .map_or(false, |s| s == ZCODE_BUNDLE_ID)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}
