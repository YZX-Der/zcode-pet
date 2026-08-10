//! 点击宠物时激活目标应用（ZCode → iTerm2 → Terminal）。

const TARGETS: &[&str] = &["ZCode", "iTerm2", "Terminal"];

#[tauri::command]
pub fn activate_target() {
    for target in TARGETS {
        let script = format!("tell application \"{target}\" to activate");
        let ok = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            break;
        }
    }
}
