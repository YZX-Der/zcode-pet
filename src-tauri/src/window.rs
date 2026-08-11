//! 窗口管理：主窗口、宠物悬浮窗的创建/定位/更新/回收。

use crate::pet;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const SPRITE_W: f64 = 192.0;
const SPRITE_H: f64 = 208.0;
const MAX_WINDOWS: usize = 5;
const MARGIN: f64 = 16.0;

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

/// 会话尚无窗口时，是否为其创建桌宠。
///
/// 仅执行中的任务（running / needs_input / blocked）创建；
/// idle（打开会话未提交任务）与 ready（任务已完成）只写状态文件，
/// 不建桌宠，避免桌面堆满「打开过」的会话宠物。
pub fn should_create_window(state: &str) -> bool {
    matches!(state, "running" | "needs_input" | "blocked")
}

/// 会话是否处于执行中（有效状态）——会话列表展示用。
pub fn is_active_state(effective: &str) -> bool {
    matches!(effective, "running" | "needs_input" | "blocked")
}

/// 为一个会话创建宠物浮窗（若尚未存在）。
pub fn ensure_window(app: &AppHandle, session_id: &str, state: &str) {
    let label = pet::session_label(session_id);

    // 已存在 → 只更新状态
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.emit("pet://state", StatePayload {
            state: effective_state(state, chrono::Utc::now().timestamp()),
        });
        return;
    }

    // 尚未有窗口：仅执行中的任务创建（idle/ready 不创建，靠衰减规则渐隐回收）
    if !should_create_window(state) {
        return;
    }

    // 用户手动禁用了该会话的桌宠 → 不创建
    if crate::settings::load()
        .disabled_sessions
        .iter()
        .any(|s| s == session_id)
    {
        log::info!("pet disabled for session {session_id}, skipping window");
        return;
    }

    // 超过上限 → 不创建
    let open_count = app
        .webview_windows()
        .keys()
        .filter(|k| k.starts_with("pet-"))
        .count();
    if open_count >= MAX_WINDOWS {
        log::warn!("pet window limit ({MAX_WINDOWS}) reached, skipping {session_id}");
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

    let effective = effective_state(state, chrono::Utc::now().timestamp());
    let scale = settings.scale;
    let opacity = settings.opacity;
    let frame_w = SPRITE_W * scale;
    let frame_h = SPRITE_H * scale;
    let position = next_position(app);

    let label_for_load = label.clone();
    let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("pet.html".into()))
        .title("")
        .inner_size(frame_w, frame_h)
        .decorations(false)
        .transparent(true)
        .always_on_top(settings.always_on_top)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(true)
        .accept_first_mouse(true)
        .on_page_load(move |webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                log::info!("pet.html loaded for '{label_for_load}'");
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
            log::info!("created pet window '{label}' ({frame_w}x{frame_h})");

            // macOS: 提升窗口级别到全屏之上，并设置跨 Space 显示。
            //
            // 注意：ensure_window 可能被 watcher 线程（后台线程）调用，
            // 而 AppKit 的 NSWindow 操作必须在主线程执行，否则触发
            // NSAssert 崩溃（SIGTRAP）。因此这里通过 run_on_main_thread
            // 调度到主线程再执行。
            #[cfg(target_os = "macos")]
            {
                let ns_window_ptr = window.ns_window().unwrap_or(std::ptr::null_mut());
                if !ns_window_ptr.is_null() {
                    let app_handle = app.clone();
                    // 裸指针不满足 Send，用 usize 传递
                    let ns_window_addr = ns_window_ptr as usize;
                    let _ = app_handle.run_on_main_thread(move || {
                        let ns_window = ns_window_addr as *mut objc::runtime::Object;
                        unsafe {
                            // setLevel: 用极高值确保在全屏窗口之上
                            // macOS 全屏窗口的 level 在 CGWindowList 里显示为 0，
                            // 但实际 NSWindow level 可能很高。用 1001 (高于 ScreenSaver)
                            let level: i64 = 1001;
                            let _: () = msg_send![ns_window, setLevel: level];
                            // setCollectionBehavior:
                            // NSWindowCollectionBehaviorCanJoinAllSpaces = 1
                            // NSWindowCollectionBehaviorStationary = 4
                            // NSWindowCollectionBehaviorFullScreenAuxiliary = 128
                            // NSWindowCollectionBehaviorIgnoresCycle = 1024
                            let behavior: u64 = 1 | 4 | 128 | 1024;
                            let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
                            // 确保不接受鼠标事件时也不被遮挡
                            let _: () = msg_send![ns_window, setOpaque: false];
                            let _: () = msg_send![ns_window, setHasShadow: false];
                        }
                    });
                }
            }
        }
        Err(e) => log::error!("failed to create window '{label}': {e}"),
    }
}

/// 关闭一个会话的窗口。
pub fn close_window(app: &AppHandle, session_id: &str) {
    let label = pet::session_label(session_id);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.close();
        log::info!("closed pet window '{label}'");
    }
}

/// 关闭一个会话：关窗 + 删状态文件 + 移出 managed state。
///
/// 若该会话在 ZCode 中仍在执行，后续事件会重新写入状态文件并恢复。
pub fn close_session(app: &AppHandle, session_id: &str) {
    close_window(app, session_id);
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
    let frame_w = SPRITE_W * settings.scale;

    let count = app
        .webview_windows()
        .keys()
        .filter(|k| k.starts_with("pet-"))
        .count() as f64;

    let x = screen_w - frame_w - MARGIN;
    let y = screen_h - frame_h - MARGIN - count * (frame_h + MARGIN);

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
                    // 同步到 managed state，确保衰减定时器/热更新/回收能跟踪这些会话
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

/// 唤醒所有窗口。
pub fn show_all(app: &AppHandle) {
    for label in all_pet_labels(app) {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.show();
        }
    }
}

/// 收起所有窗口。
pub fn hide_all(app: &AppHandle) {
    for label in all_pet_labels(app) {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.hide();
        }
    }
}

/// 更新所有宠物窗口的参数（设置变更后，不重建窗口）。
pub fn recreate_all(app: &AppHandle) {
    let settings = crate::settings::load();
    let scale = settings.scale;
    let frame_w = SPRITE_W * scale;
    let frame_h = SPRITE_H * scale;
    let pet_name = settings.pet.clone();
    let opacity = settings.opacity;
    let sheet = pet::sheet_path(&pet_name)
        .unwrap_or_else(|| pet::pet_dir("zbuddy").join("spritesheet.webp"));

    // 收集每个会话的当前有效状态（避免热更新时全部回退成 idle）
    let session_states: Vec<(String, String)> = {
        let app_state = app.state::<crate::AppState>();
        let sessions = app_state.sessions.lock().unwrap();
        sessions
            .iter()
            .map(|(sid, entry)| (sid.clone(), effective_state(&entry.raw_state, entry.ts)))
            .collect()
    };

    for (session_id, state) in session_states {
        let label = pet::session_label(&session_id);
        if let Some(window) = app.get_webview_window(&label) {
            // 修改窗口尺寸
            use tauri::LogicalSize;
            let _ = window.set_size(LogicalSize::new(frame_w, frame_h));
            let _ = window.set_always_on_top(settings.always_on_top);

            // 注入新参数并调用前端的 updatePet（状态保留会话自己的有效状态）
            let init_js = format!(
                r#"window.__PET_INIT__ = {{ pet: "{pet_name}", state: "{state}", sheet: "{}", scale: {scale}, opacity: {opacity} }};
                if (typeof window.updatePet === 'function') window.updatePet();"#,
                sheet.display()
            );
            let _ = window.eval(&init_js);

            log::info!("updated pet window '{label}' ({frame_w}x{frame_h}) state={state}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{compute_effective_state, is_active_state, should_create_window};

    const NOW: i64 = 10_000;

    #[test]
    fn test_only_active_states_create_window() {
        assert!(should_create_window("running"));
        assert!(should_create_window("needs_input"));
        assert!(should_create_window("blocked"));
        // 打开会话/任务完成/休眠不创建新窗口（已有窗口则继续更新）
        assert!(!should_create_window("idle"));
        assert!(!should_create_window("ready"));
        assert!(!should_create_window("sleep"));
    }

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
