//! 点击宠物时激活 ZCode 应用到前台。

#[tauri::command]
pub fn activate_target() {
    let _ = std::process::Command::new("osascript")
        .args(["-e", "tell application \"ZCode\" to activate"])
        .output();
}
