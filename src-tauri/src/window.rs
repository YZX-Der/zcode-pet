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
/// 空闲状态按用户配置的变淡延迟衰减，任务状态保持原有 600s 规则。
pub fn effective_state(raw_state: &str, ts: i64) -> String {
    let idle_fade = crate::settings::load().idle_fade_seconds as i64;
    compute_effective_state(raw_state, ts, chrono::Utc::now().timestamp(), idle_fade)
}

/// 可测试的纯函数版本。
pub fn compute_effective_state(raw_state: &str, ts: i64, now: i64, idle_fade_seconds: i64) -> String {
    let elapsed = now - ts;
    // R1: ready 持续 300s -> idle；此后按空闲规则变淡（从进入 idle 起算）。
    // 修复原逻辑：ready 衰减成空闲后永不衰减的问题。
    if raw_state == "ready" && elapsed > 300 {
        if idle_fade_seconds > 0 && now - (ts + 300) > idle_fade_seconds {
            return "sleep".into();
        }
        return "idle".into();
    }
    if raw_state == "idle" {
        // R2（空闲）：按可配置延迟变淡；0 = 关闭
        if idle_fade_seconds > 0 && elapsed > idle_fade_seconds {
            return "sleep".into();
        }
    } else if elapsed > 600 {
        // R2（任务状态）：保持原有 600s 规则
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
                            // 非聚焦窗口也接收鼠标移动事件：
                            // 否则 macOS 默认不向非 key window 派发 hover，
                            // 重启后未点击聚焦时气泡无法通过移入显示
                            let _: () = msg_send![ns_window, setAcceptsMouseMovedEvents: true];
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

/// 权限确认弹窗窗口 label。
const REQUEST_LABEL: &str = "pet-request";
/// 确认弹窗尺寸（逻辑像素）。
const REQUEST_W: f64 = 280.0;
const REQUEST_H: f64 = 180.0;

/// 将确认弹窗定位到状态气泡框下方（气泡在桌宠窗口内右侧，
/// 弹窗对齐气泡框左缘、紧贴其下方；右侧/底部空间不足时回退到桌宠左侧/气泡上方）。
fn position_request_near_pet(window: &tauri::WebviewWindow, app: &AppHandle) {
    let Some(pet) = app.get_webview_window(PET_LABEL) else { return };
    let Ok(pet_pos) = pet.outer_position() else { return };
    let Some(monitor) = app.primary_monitor().ok().flatten() else { return };
    let sf = monitor.scale_factor();
    let screen_w = monitor.size().width as f64 / sf;
    let screen_h = monitor.size().height as f64 / sf;

    let pet_x = pet_pos.x as f64 / sf;
    let pet_y = pet_pos.y as f64 / sf;

    // 气泡框在桌宠窗口内的位置：canvas 宽（192×scale）+ 6px 起，顶部 6px，高约 30px
    let pet_scale = crate::settings::load().scale;
    let canvas_w = 192.0 * pet_scale;
    const BUBBLE_TOP: f64 = 6.0;
    const BUBBLE_H: f64 = 30.0;
    let bubble_x = pet_x + canvas_w + 6.0;
    let bubble_bottom = pet_y + BUBBLE_TOP + BUBBLE_H;

    // 弹窗对齐气泡框左缘，放在气泡正下方。
    // 弹窗起点在精灵右侧（canvas 宽之外），不会压住宠物本体，
    // 覆盖的只是桌宠窗口的透明区域，因此无需再避让桌宠窗口。
    let mut x = bubble_x;
    let mut y = bubble_bottom + 4.0;

    // 右侧超出屏幕 -> 弹窗放到桌宠左侧
    if x + REQUEST_W > screen_w {
        x = (pet_x - REQUEST_W - 8.0).max(0.0);
    }
    // 底部超出屏幕 -> 弹窗放到气泡上方
    if y + REQUEST_H > screen_h {
        y = (pet_y - REQUEST_H - 6.0).max(0.0);
    }

    let _ = window.set_position(tauri::LogicalPosition::new(x, y));
}

/// 显示权限确认弹窗（不存在则创建）。
/// 创建时用 visible(false) + show()，不 set_focus——避免激活应用导致主窗口前置。
pub fn show_request_window(app: &AppHandle) {
    // 已存在 -> 重新定位（宠物可能被拖动过）再显示（不聚焦，不打扰）
    if let Some(window) = app.get_webview_window(REQUEST_LABEL) {
        position_request_near_pet(&window, app);
        let _ = window.show();
        return;
    }

    let builder = WebviewWindowBuilder::new(app, REQUEST_LABEL, WebviewUrl::App("request.html".into()))
        .title("")
        .inner_size(REQUEST_W, REQUEST_H)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .accept_first_mouse(false)
        .on_page_load(move |webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                log::info!("request.html loaded");
                let _ = webview.eval("window.__REQUEST_READY__ = true; if (typeof window.initRequest === 'function') window.initRequest();");
            }
        });

    match builder.build() {
        Ok(window) => {
            position_request_near_pet(&window, app);
            // 跨 Space 显示：否则切换全屏 Space 时弹窗会消失（objc 必须在主线程）
            #[cfg(target_os = "macos")]
            {
                let ns_window_ptr = window.ns_window().unwrap_or(std::ptr::null_mut());
                if !ns_window_ptr.is_null() {
                    let app_handle = app.clone();
                    let ns_window_addr = ns_window_ptr as usize;
                    let _ = app_handle.run_on_main_thread(move || {
                        let ns_window = ns_window_addr as *mut objc::runtime::Object;
                        unsafe {
                            // CanJoinAllSpaces=1 | Stationary=4 | FullScreenAuxiliary=128 | IgnoresCycle=1024
                            let behavior: u64 = 1 | 4 | 128 | 1024;
                            let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
                        }
                    });
                }
            }
            // show() 不激活应用，避免主窗口被前置
            let _ = window.show();
            log::info!("created request window");
        }
        Err(e) => log::error!("failed to create request window: {e}"),
    }
}

/// 关闭权限确认弹窗。
pub fn close_request_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(REQUEST_LABEL) {
        let _ = window.close();
        log::info!("closed request window");
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
    const IDLE_FADE: i64 = 600; // 默认空闲变淡延迟

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
        assert_eq!(compute_effective_state("running", NOW - 10, NOW, IDLE_FADE), "running");
    }

    #[test]
    fn test_needs_input_stays() {
        assert_eq!(
            compute_effective_state("needs_input", NOW - 100, NOW, IDLE_FADE),
            "needs_input"
        );
    }

    #[test]
    fn test_ready_within_300s_stays_ready() {
        assert_eq!(compute_effective_state("ready", NOW - 300, NOW, IDLE_FADE), "ready");
    }

    #[test]
    fn test_ready_after_300s_decays_to_idle() {
        assert_eq!(compute_effective_state("ready", NOW - 301, NOW, IDLE_FADE), "idle");
    }

    #[test]
    fn test_any_state_after_600s_decays_to_sleep() {
        assert_eq!(compute_effective_state("running", NOW - 601, NOW, IDLE_FADE), "sleep");
        assert_eq!(compute_effective_state("blocked", NOW - 601, NOW, IDLE_FADE), "sleep");
    }

    #[test]
    fn test_ready_exactly_300s_stays_ready() {
        assert_eq!(compute_effective_state("ready", NOW - 300, NOW, IDLE_FADE), "ready");
    }

    #[test]
    fn test_any_state_exactly_600s_not_sleep() {
        assert_eq!(compute_effective_state("idle", NOW - 600, NOW, IDLE_FADE), "idle");
    }

    #[test]
    fn test_idle_fades_after_configured_delay() {
        // 自定义延迟 60s：61s 后变淡
        assert_eq!(compute_effective_state("idle", NOW - 61, NOW, 60), "sleep");
        // 60s 内保持空闲
        assert_eq!(compute_effective_state("idle", NOW - 60, NOW, 60), "idle");
    }

    #[test]
    fn test_idle_fade_disabled_stays_idle() {
        // 0 = 关闭空闲变淡，空闲永不衰减
        assert_eq!(compute_effective_state("idle", NOW - 3600, NOW, 0), "idle");
    }

    #[test]
    fn test_task_states_ignore_idle_fade_setting() {
        // 任务状态不受空闲延迟配置影响，仍按 600s
        assert_eq!(compute_effective_state("running", NOW - 61, NOW, 60), "running");
        assert_eq!(compute_effective_state("running", NOW - 601, NOW, 60), "sleep");
    }

    #[test]
    fn test_ready_decays_to_idle_then_sleeps_per_idle_fade() {
        // ready 300s 后变 idle；从进入 idle 起算 idle_fade 后变 sleep（修复永不衰减）
        assert_eq!(compute_effective_state("ready", NOW - 301, NOW, 600), "idle");
        // 进入 idle 600s 整仍为 idle，601s 后 sleep
        assert_eq!(compute_effective_state("ready", NOW - 900, NOW, 600), "idle");
        assert_eq!(compute_effective_state("ready", NOW - 901, NOW, 600), "sleep");
        // idle_fade=60：ready 361s（300+60+1）后变 sleep
        assert_eq!(compute_effective_state("ready", NOW - 361, NOW, 60), "sleep");
        // 关闭（0）：ready 变 idle 后不再衰减
        assert_eq!(compute_effective_state("ready", NOW - 3600, NOW, 0), "idle");
    }
}
