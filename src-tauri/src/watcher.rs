//! 状态目录监听器 + 衰减定时器。
//!
//! 监听 ~/.zcode-pet/state/ 变化 -> 读文件 -> 创建/更新窗口；
//! 每 60s 扫描一次应用衰减规则并回收失联会话。

use crate::pet;
use crate::window;
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// 处理单个状态文件变化。
fn handle_state_file(app: &AppHandle, path: &Path) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(state) = serde_json::from_str::<pet::StateFile>(&content) else {
        return;
    };

    // 更新 managed state
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

    window::ensure_window(app, &state.session_id, &state.state, true);
    // 权限请求弹窗只由当前会话控制：
    // 其他会话的状态变化（如正在执行的真实会话持续写 running）不干扰弹窗开关
    if pet::current_session_id().as_deref() == Some(state.session_id.as_str()) {
        if state.state == "needs_input" {
            // 用户已在 ZCode 窗口时只保留气泡提示，不弹确认窗（避免遮挡权限框）
            if !crate::activate::is_zcode_frontmost() {
                window::show_request_window(app);
            }
        } else {
            window::close_request_window(app);
        }
    }
    // 刷新托盘菜单（当前会话信息区实时更新）
    crate::refresh_tray_menu(app);
}

/// 启动文件监听器（独立线程，debounce 200ms）。
pub fn start_watcher(app: AppHandle) {
    let state_dir = pet::state_dir();
    std::fs::create_dir_all(&state_dir).ok();

    let app_handle = app.clone();

    std::thread::spawn(move || {
        use notify::RecursiveMode;
        use notify_debouncer_full::new_debouncer;

        let handler_app = app_handle.clone();
        let mut debouncer = match new_debouncer(
            Duration::from_millis(200),
            None,
            move |result: notify_debouncer_full::DebounceEventResult| {
                if let Ok(events) = result {
                    for event in events {
                        for path in &event.event.paths {
                            if path.extension().map_or(false, |e| e == "json") {
                                handle_state_file(&handler_app, path);
                            }
                        }
                    }
                }
            },
        ) {
            Ok(d) => d,
            Err(e) => {
                log::error!("watcher init failed: {e}");
                return;
            }
        };

        if let Err(e) = debouncer
            .watch(&state_dir, RecursiveMode::NonRecursive)
        {
            log::error!("watcher start failed: {e}");
            return;
        }

        log::info!("watching {}", state_dir.display());

        // 阻塞线程以保持 debouncer 存活
        // notify_debouncer_full 内部线程持续运行，这里 sleep 保活
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    });
}

/// 启动衰减定时器（每 60s 扫描一次）。
pub fn start_decay_timer(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60));
        apply_decay(&app);
    });
}

/// 应用衰减规则：更新窗口状态、回收失联会话。
fn apply_decay(app: &AppHandle) {
    let now = chrono::Utc::now().timestamp();
    // 当前活跃会话豁免回收：即使长时间无事件也保持（变淡显示），不消失
    let current_sid = pet::current_session_id();
    let mut dead_sessions = Vec::new();

    {
        let app_state = app.state::<crate::AppState>();
        let mut sessions = app_state.sessions.lock().unwrap();

        // 衰减后当前会话的有效状态（单一桌宠只更新这一个）
        let mut current_effective: Option<String> = None;

        for (session_id, entry) in sessions.iter_mut() {
            let effective = window::effective_state(&entry.raw_state, entry.ts);

            // R3: 1800s 无事件 -> 会话死亡（当前会话豁免，保持淡显示不回收）
            if now - entry.ts > 1800 && current_sid.as_deref() != Some(session_id.as_str()) {
                dead_sessions.push(session_id.clone());
                continue;
            }

            // 只更新当前会话的桌宠状态
            if current_sid.as_deref() == Some(session_id.as_str()) {
                current_effective = Some(effective);
            }
        }

        // 更新桌宠窗口状态（单一桌宠反映当前会话）
        if let Some(state) = current_effective {
            if let Some(window) = app.get_webview_window("pet") {
                let _ = window.emit(
                    "pet://state",
                    window::StatePayload { state },
                );
            }
        }

        for sid in &dead_sessions {
            sessions.remove(sid);
        }
    }

    // 清理死亡会话的状态文件（单一桌宠模式下不需要关窗，桌宠只跟随当前会话）
    for sid in &dead_sessions {
        let file = pet::state_dir().join(format!("{sid}.json"));
        let _ = std::fs::remove_file(&file);
        log::info!("reaped dead session {sid}");
    }

    // 刷新托盘菜单（Token 用量等可能在无 hook 事件时变化，定时兜底）
    crate::refresh_tray_menu(app);
}
