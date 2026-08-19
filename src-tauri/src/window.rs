//! 窗口管理：主窗口、宠物悬浮窗的创建/定位/更新/回收。

use crate::pet;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const SPRITE_W: f64 = 192.0;
const SPRITE_H: f64 = 208.0;
/// 状态气泡区宽度（固定逻辑像素，不随 scale 缩放）
const BUBBLE_W: f64 = 96.0;
const MARGIN: f64 = 16.0;
/// 单一桌宠窗口固定 label
const PET_LABEL: &str = "pet";

#[derive(Serialize, Clone)]
pub struct StatePayload {
    pub state: String,
}

/// 根据原始状态与时间戳计算有效状态（含衰减规则）。
pub fn effective_state(raw_state: &str, ts: i64) -> String {
    compute_effective_state(raw_state, ts, chrono::Utc::now().timestamp())
}

/// 可测试的纯函数版本。
pub fn compute_effective_state(raw_state: &str, ts: i64, now: i64) -> String {
    let elapsed = now - ts;
    if raw_state == "ready" && elapsed > 300 {
        return "idle".into();
    }
    if elapsed > 600 {
        return "sleep".into();
    }
    raw_state.to_string()
}

/// 显示主窗口（已存在则聚焦）。
pub fn show_main_window(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        // NSApp activateIgnoringOtherApps:YES
        unsafe {
            let cls = objc::class!(NSApplication);
            let ns_app: *mut objc::runtime::Object = msg_send![cls, sharedApplication];
            if !ns_app.is_null() {
                let _: () = msg_send![ns_app, activateIgnoringOtherApps: true];
            }
        }
    }

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// 会话是否处于执行中（有效状态）——会话列表展示用。
pub fn is_active_state(effective: &str) -> bool {
    matches!(effective, "running" | "needs_input" | "blocked")
}

/// 确保桌宠窗口存在并反映给定状态（单一桌宠模式）。
///
/// 窗口已存在 -> 只 emit 更新状态（切换会话时不重建窗口，无闪烁）。
/// 窗口不存在 -> 创建固定 label 的桌宠窗口。
/// 全局开关关闭（pet_hidden）时不创建/不显示。
pub fn ensure_window(app: &AppHandle, _session_id: &str, state: &str, _force: bool) {
    let effective = effective_state(state, chrono::Utc::now().timestamp());

    // 窗口已存在 -> 只更新状态（复用窗口，不重建）
    if let Some(window) = app.get_webview_window(PET_LABEL) {
        let _ = window.emit("pet://state", StatePayload { state: effective });
        return;
    }

    // 全局开关关闭 -> 不创建
    if crate::settings::load().pet_hidden {
        log::info!("pet globally hidden, skipping window creation");
        return;
    }

    let settings = crate::settings::load();
    let pet_name = if settings.pet.is_empty() {
        app.state::<crate::AppState>()
            .pet_name
            .lock()
            .unwrap()
            .clone()
    } else {
        settings.pet.clone()
    };

    let sheet = pet::sheet_path(&pet_name)
        .unwrap_or_else(|| pet::pet_dir("zbuddy").join("spritesheet.webp"));

    let scale = settings.scale;
    let opacity = settings.opacity;
    // 窗口宽 = 宠物宽 + 气泡区（固定 110 逻辑像素）
    let frame_w = SPRITE_W * scale + BUBBLE_W;
    let frame_h = SPRITE_H * scale;
    let position = next_position(app);

    let builder = WebviewWindowBuilder::new(app, PET_LABEL, WebviewUrl::App("pet.html".into()))
        .title("")
        .inner_size(frame_w, frame_h)
        .decorations(false)
        .transparent(true)
        .always_on_top(settings.always_on_top)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(true)
        // 第一次点击只聚焦窗口（不触发跳转/事件），第二次点击才响应——
        // 避免误触跳转，且聚焦后 hover 事件（气泡显示）才生效
        .accept_first_mouse(false)
        .on_page_load(move |webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                log::info!("pet.html loaded");
                let init_js = format!(
                    r#"window.__PET_INIT__ = {{ pet: "{pet_name}", state: "{effective}", sheet: "{}", scale: {scale}, opacity: {opacity} }};"#,
                    sheet.display()
                );
                let _ = webview.eval(&init_js);
                let _ = webview.eval("if (typeof window.initPet === 'function') window.initPet();");
            }
        });

    let builder = if let Some((x, y)) = position {
        builder.position(x, y)
    } else {
        builder
    };

    match builder.build() {
        Ok(window) => {
            log::info!("created pet window ({frame_w}x{frame_h})");

            #[cfg(target_os = "macos")]
            {
                let ns_window_ptr = window.ns_window().unwrap_or(std::ptr::null_mut());
                if !ns_window_ptr.is_null() {
                    let app_handle = app.clone();
                    let ns_window_addr = ns_window_ptr as usize;
                    let _ = app_handle.run_on_main_thread(move || {
                        let ns_window = ns_window_addr as *mut objc::runtime::Object;
                        unsafe {
                            let level: i64 = 1001;
                            let _: () = msg_send![ns_window, setLevel: level];
                            let behavior: u64 = 1 | 4 | 128 | 1024;
                            let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
                            let _: () = msg_send![ns_window, setOpaque: false];
                            let _: () = msg_send![ns_window, setHasShadow: false];
                        }
                    });
                }
            }
        }
        Err(e) => log::error!("failed to create pet window: {e}"),
    }
}

/// 关闭桌宠窗口。
pub fn close_pet_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PET_LABEL) {
        let _ = window.close();
        log::info!("closed pet window");
    }
}

/// 显示/隐藏桌宠窗口（全局开关）。
pub fn set_pet_visible(app: &AppHandle, visible: bool) {
    if visible {
        if let Some(window) = app.get_webview_window(PET_LABEL) {
            let _ = window.show();
        } else {
            // 窗口不存在 -> 用当前会话状态创建
            let current = pet::current_session_id();
            if let Some(sid) = &current {
                let state_file = pet::state_dir().join(format!("{sid}.json"));
                if let Ok(content) = std::fs::read_to_string(&state_file) {
                    if let Ok(sf) = serde_json::from_str::<pet::StateFile>(&content) {
                        ensure_window(app, &sf.session_id, &sf.state, true);
                        return;
                    }
                }
                // 无状态文件 -> 用 idle 创建
                ensure_window(app, sid, "idle", true);
            }
        }
    } else {
        if let Some(window) = app.get_webview_window(PET_LABEL) {
            let _ = window.hide();
        }
    }
}

/// 关闭一个会话：删状态文件 + 移出 managed state。
///
/// 若该会话在 ZCode 中仍在执行，后续事件会重新写入状态文件并恢复。
pub fn close_session(app: &AppHandle, session_id: &str) {
    let file = pet::state_dir().join(format!("{session_id}.json"));
    let _ = std::fs::remove_file(&file);
    {
        let app_state = app.state::<crate::AppState>();
        let mut sessions = app_state.sessions.lock().unwrap();
        sessions.remove(session_id);
    }
    log::info!("closed session {session_id}");
}

/// 计算下一个窗口位置：从右下角向上堆叠。
fn next_position(app: &AppHandle) -> Option<(f64, f64)> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let size = monitor.size();
    let scale = monitor.scale_factor();

    let screen_w = size.width as f64 / scale;
    let screen_h = size.height as f64 / scale;

    let settings = crate::settings::load();
    let frame_h = SPRITE_H * settings.scale;
    let frame_w = SPRITE_W * settings.scale + BUBBLE_W;

    // 固定右下角
    let x = screen_w - frame_w - MARGIN;
    let y = screen_h - frame_h - MARGIN;

    Some((x, y))
}

/// 扫描现有状态文件，为当前会话创建桌宠窗口（单一桌宠模式）。
pub fn scan_and_create(app: &AppHandle) {
    let state_dir = pet::state_dir();
    let current_sid = pet::current_session_id();
    if let Ok(entries) = std::fs::read_dir(&state_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "json") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(state) = serde_json::from_str::<pet::StateFile>(&content) {
                    // 同步到 managed state
                    {
                        let app_state = app.state::<crate::AppState>();
                        let mut sessions = app_state.sessions.lock().unwrap();
                        sessions.insert(
                            state.session_id.clone(),
                            crate::SessionEntry {
                                raw_state: state.state.clone(),
                                ts: state.ts,
                            },
                        );
                    }
                    // 单一桌宠模式：启动时只为当前活跃会话创建窗口
                    if current_sid.as_deref() == Some(state.session_id.as_str()) {
                        ensure_window(app, &state.session_id, &state.state, true);
                    }
                }
            }
        }
    }
}

/// 唤醒桌宠窗口。
pub fn show_all(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PET_LABEL) {
        let _ = window.show();
    }
}

/// 收起桌宠窗口。
pub fn hide_all(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PET_LABEL) {
        let _ = window.hide();
    }
}

/// 更新桌宠窗口参数（设置变更后，不重建窗口）。
pub fn recreate_all(app: &AppHandle) {
    let settings = crate::settings::load();
    let scale = settings.scale;
    let frame_w = SPRITE_W * scale + BUBBLE_W;
    let frame_h = SPRITE_H * scale;
    let pet_name = settings.pet.clone();
    let opacity = settings.opacity;
    let sheet = pet::sheet_path(&pet_name)
        .unwrap_or_else(|| pet::pet_dir("zbuddy").join("spritesheet.webp"));

    if let Some(window) = app.get_webview_window(PET_LABEL) {
        use tauri::LogicalSize;
        let _ = window.set_size(LogicalSize::new(frame_w, frame_h));
        let _ = window.set_always_on_top(settings.always_on_top);

        // 当前会话的有效状态
        let state = pet::current_session_id()
            .and_then(|sid| {
                let app_state = app.state::<crate::AppState>();
                let sessions = app_state.sessions.lock().unwrap();
                sessions.get(&sid).map(|e| effective_state(&e.raw_state, e.ts))
            })
            .unwrap_or_else(|| "idle".to_string());

        let init_js = format!(
            r#"window.__PET_INIT__ = {{ pet: "{pet_name}", state: "{state}", sheet: "{}", scale: {scale}, opacity: {opacity} }};
            if (typeof window.updatePet === 'function') window.updatePet();"#,
            sheet.display()
        );
        let _ = window.eval(&init_js);

        log::info!("updated pet window ({frame_w}x{frame_h}) state={state}");
    }
}

#[cfg(test)]
mod tests {
    use super::{compute_effective_state, is_active_state};

    const NOW: i64 = 10_000;

    #[test]
    fn test_active_state_list_filter() {
        assert!(is_active_state("running"));
        assert!(is_active_state("needs_input"));
        assert!(is_active_state("blocked"));
        assert!(!is_active_state("idle"));
        assert!(!is_active_state("ready"));
        assert!(!is_active_state("sleep"));
    }

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
        assert_eq!(compute_effective_state("ready", NOW - 301, NOW), "idle");
    }

    #[test]
    fn test_any_state_after_600s_decays_to_sleep() {
        assert_eq!(compute_effective_state("running", NOW - 601, NOW), "sleep");
        assert_eq!(compute_effective_state("blocked", NOW - 601, NOW), "sleep");
    }

    #[test]
    fn test_ready_exactly_300s_stays_ready() {
        assert_eq!(compute_effective_state("ready", NOW - 300, NOW), "ready");
    }

    #[test]
    fn test_any_state_exactly_600s_not_sleep() {
        assert_eq!(compute_effective_state("idle", NOW - 600, NOW), "idle");
    }
}
