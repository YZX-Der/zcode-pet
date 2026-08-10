//! 宠物窗口管理：创建、定位、状态更新、回收。

use crate::pet;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const FRAME_W: f64 = 192.0;
const FRAME_H: f64 = 208.0;
const MAX_WINDOWS: usize = 5;
const MARGIN: f64 = 16.0;

#[derive(Serialize, Clone)]
pub struct StatePayload {
    pub state: String,
}

/// 根据原始状态与时间戳计算有效状态（含衰减规则）。
///
/// 规则见 docs/03-state-protocol.md §4
pub fn effective_state(raw_state: &str, ts: i64) -> String {
    compute_effective_state(raw_state, ts, chrono::Utc::now().timestamp())
}

/// 可测试的纯函数版本：传入 now 避免依赖系统时钟。
pub fn compute_effective_state(raw_state: &str, ts: i64, now: i64) -> String {
    let elapsed = now - ts;

    // R1: ready 持续 300s → idle
    if raw_state == "ready" && elapsed > 300 {
        return "idle".into();
    }
    // R2: 任意状态 600s → sleep（不写文件，仅推导）
    if elapsed > 600 {
        return "sleep".into();
    }
    raw_state.to_string()
}

/// 为一个会话创建宠物窗口（若尚未存在）。
pub fn ensure_window(app: &AppHandle, session_id: &str, state: &str) {
    let label = pet::session_label(session_id);

    // 已存在 → 只更新状态
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.emit("pet://state", StatePayload {
            state: effective_state(state, chrono::Utc::now().timestamp()),
        });
        return;
    }

    // 超过上限 → 不创建（聚合策略后续迭代）
    let open_count = app
        .webview_windows()
        .keys()
        .filter(|k| k.starts_with("pet-"))
        .count();
    if open_count >= MAX_WINDOWS {
        log::warn!("pet window limit ({MAX_WINDOWS}) reached, skipping {session_id}");
        return;
    }

    let pet_name = app
        .state::<crate::AppState>()
        .pet_name
        .lock()
        .unwrap()
        .clone();

    let sheet = pet::sheet_path(&pet_name)
        .unwrap_or_else(|| pet::pet_dir("zbuddy").join("spritesheet.webp"));

    let effective = effective_state(state, chrono::Utc::now().timestamp());
    let position = next_position(app);

    let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("")
        .inner_size(FRAME_W, FRAME_H)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(true);

    let builder = if let Some((x, y)) = position {
        builder.position(x, y)
    } else {
        builder
    };

    match builder.build() {
        Ok(window) => {
            // 通过 JS 注入参数（避免 URL 查询字符串的路径转义问题）
            let init_js = format!(
                r#"window.__PET_INIT__ = {{ pet: "{pet_name}", state: "{effective}", sheet: "{}" }};"#,
                sheet.display()
            );
            let _ = window.eval(&init_js);
            log::info!("created pet window '{label}' for session {session_id}");
        }
        Err(e) => log::error!("failed to create window '{label}': {e}"),
    }
}

/// 关闭并清理一个会话的窗口。
pub fn close_window(app: &AppHandle, session_id: &str) {
    let label = pet::session_label(session_id);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.close();
        log::info!("closed pet window '{label}'");
    }
}

/// 计算下一个窗口位置：从右下角向上堆叠。
fn next_position(app: &AppHandle) -> Option<(f64, f64)> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let size = monitor.size();
    let scale = monitor.scale_factor();

    let screen_w = size.width as f64 / scale;
    let screen_h = size.height as f64 / scale;

    let count = app
        .webview_windows()
        .keys()
        .filter(|k| k.starts_with("pet-"))
        .count() as f64;

    let x = screen_w - FRAME_W - MARGIN;
    let y = screen_h - FRAME_H - MARGIN - count * (FRAME_H + MARGIN);

    Some((x, y))
}

/// 扫描现有状态文件并为每个活跃会话创建窗口。
pub fn scan_and_create(app: &AppHandle) {
    let state_dir = pet::state_dir();
    if let Ok(entries) = std::fs::read_dir(&state_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "json") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(state) = serde_json::from_str::<pet::StateFile>(&content) {
                    ensure_window(app, &state.session_id, &state.state);
                }
            }
        }
    }
}

/// 获取所有宠物窗口的 label 列表。
pub fn all_pet_labels(app: &AppHandle) -> Vec<String> {
    app.webview_windows()
        .keys()
        .filter(|k| k.starts_with("pet-"))
        .cloned()
        .collect()
}

/// 唤醒所有窗口（显示）。
pub fn show_all(app: &AppHandle) {
    for label in all_pet_labels(app) {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.show();
        }
    }
}

/// 收起所有窗口（隐藏）。
pub fn hide_all(app: &AppHandle) {
    for label in all_pet_labels(app) {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.hide();
        }
    }
}

/// 让所有窗口切换到指定状态（用于托盘"测试"功能或唤醒动画）。
pub fn set_all_state(app: &AppHandle, state: &str) {
    for label in all_pet_labels(app) {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.emit(
                "pet://state",
                StatePayload { state: state.into() },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::compute_effective_state;

    const NOW: i64 = 10_000;

    #[test]
    fn test_running_stays_running() {
        assert_eq!(compute_effective_state("running", NOW - 10, NOW), "running");
    }

    #[test]
    fn test_needs_input_stays() {
        assert_eq!(
            compute_effective_state("needs_input", NOW - 100, NOW),
            "needs_input"
        );
    }

    #[test]
    fn test_ready_within_300s_stays_ready() {
        assert_eq!(compute_effective_state("ready", NOW - 300, NOW), "ready");
    }

    #[test]
    fn test_ready_after_300s_decays_to_idle() {
        // R1: ready > 300s → idle
        assert_eq!(compute_effective_state("ready", NOW - 301, NOW), "idle");
    }

    #[test]
    fn test_any_state_after_600s_decays_to_sleep() {
        // R2: 任意非 ready 状态 > 600s → sleep
        assert_eq!(compute_effective_state("running", NOW - 601, NOW), "sleep");
        assert_eq!(compute_effective_state("blocked", NOW - 601, NOW), "sleep");
        // 注意：ready > 300s 先命中 R1 返回 idle，不会走到 R2
    }

    #[test]
    fn test_ready_exactly_300s_stays_ready() {
        // 边界：恰好 300s 不衰减
        assert_eq!(compute_effective_state("ready", NOW - 300, NOW), "ready");
    }

    #[test]
    fn test_any_state_exactly_600s_not_sleep() {
        // 边界：恰好 600s 不进入 sleep
        assert_eq!(compute_effective_state("idle", NOW - 600, NOW), "idle");
    }
}
